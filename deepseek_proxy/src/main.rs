mod admin;
mod auth;
mod config;
mod error;
mod deepseek;
mod proxy;
mod quota;
mod utils;

use auth::{login, auth_middleware, JwtService};
use axum::{
    middleware,
    routing::post,
    Router,
};
use config::Config;
use deepseek::DeepSeekClient;
use proxy::{proxy_chat, LoginLimiter};
use quota::QuotaManager;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// 统一的应用状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub jwt_service: Arc<JwtService>,
    pub deepseek_client: Arc<DeepSeekClient>,
    pub login_limiter: Arc<LoginLimiter>, // 现在统一管理Token生命周期和并发控制
    pub quota_manager: Arc<QuotaManager>,
    pub user_manager: Arc<auth::UserManager>, // 用户管理器（内存+持久化）
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志（使用东八区时间）
    let timer = tracing_subscriber::fmt::time::OffsetTime::new(
        time::UtcOffset::from_hms(8, 0, 0).expect("Invalid UTC offset"),
        time::format_description::well_known::Rfc3339,
    );

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "deepseek_proxy=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_timer(timer))
        .init();

    // 加载配置
    let config = Config::load()?;
    tracing::info!("配置加载成功");
    tracing::info!("服务器地址: {}:{}", config.server.host, config.server.port);
    tracing::info!("DeepSeek API: {}", config.deepseek.base_url);
    tracing::info!("限流: 每个 token 同时只允许1个请求");
    tracing::info!("登录: 每个用户每 {} 秒只能登录1次", config.auth.token_ttl_seconds.min(60));
    tracing::info!("HTTP客户端: 连接池={}个, 保活={}秒, 连接超时={}秒", 
        config.deepseek.http_client.pool_max_idle_per_host,
        config.deepseek.http_client.pool_idle_timeout_seconds,
        config.deepseek.http_client.connect_timeout_seconds
    );

    // 初始化组件
    let jwt_service = Arc::new(JwtService::new(
        config.auth.jwt_secret.clone(),
        config.auth.token_ttl_seconds,
    ).map_err(|e| anyhow::anyhow!("JWT服务初始化失败: {}", e))?);

    let deepseek_client = Arc::new(DeepSeekClient::new(
        config.deepseek.api_key.clone(),
        config.deepseek.base_url.clone(),
        config.deepseek.timeout_seconds,
        &config.deepseek.http_client,
    ).map_err(|e| anyhow::anyhow!("DeepSeek客户端初始化失败: {}", e))?);

    let login_limiter = Arc::new(LoginLimiter::new(config.auth.token_ttl_seconds));

    // 初始化用户管理器（基于文件存储）- 必须在配额管理器之前
    let users_dir = PathBuf::from("data/users");
    let user_manager = Arc::new(
        auth::UserManager::new(users_dir, config.auth.users.clone())
            .await
            .map_err(|e| anyhow::anyhow!("用户管理器初始化失败: {}", e))?
    );
    tracing::info!("用户管理器初始化完成，用户数据存储在 data/users/");

    // 初始化配额管理器（需要 user_manager 来查询动态用户）
    let data_dir = PathBuf::from("data/quotas");
    tokio::fs::create_dir_all(&data_dir).await?;
    let config_arc = Arc::new(config.clone());
    let quota_manager = Arc::new(QuotaManager::new(
        config_arc,
        user_manager.clone(),
        data_dir,
        config.quota.save_interval,
    ));

    tracing::info!("配额: 每 {} 次请求写一次磁盘", config.quota.save_interval);

    let config = Arc::new(config);

    // 创建统一的应用状态
    let app_state = AppState {
        config: config.clone(),
        jwt_service,
        deepseek_client,
        login_limiter, // 统一管理Token生命周期和并发控制
        quota_manager: quota_manager.clone(),
        user_manager,
    };

    // 构建路由
    // 公开路由（无需认证）
    let public_routes = Router::new()
        .route("/auth/login", post(login));

    // 受保护路由（需要 Token）
    let protected_routes = Router::new()
        .route("/chat/completions", post(proxy_chat))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth_middleware,
        ));

    // 管理路由（只允许 localhost 访问）
    let admin_routes = Router::new()
        .route("/admin/users/:username/active", post(admin::set_user_active))
        .route("/admin/users/:username", axum::routing::get(admin::get_user))
        .route("/admin/users",
            axum::routing::get(admin::list_users)
                .post(admin::create_user)
        )
        .layer(middleware::from_fn(admin::localhost_only))
        .with_state(app_state.clone());

    // 合并路由
    let app = public_routes
        .merge(protected_routes)
        .merge(admin_routes)
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());

    // 启动服务器
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("🚀 DeepSeek 代理服务启动成功: http://{}", addr);
    tracing::info!("📝 登录接口: POST http://{}/auth/login", addr);
    tracing::info!("🔄 代理接口: POST http://{}/chat/completions", addr);
    tracing::info!("🔧 管理接口: POST http://{}/admin/users/{{username}}/active (仅localhost)", addr);

    // 优雅关闭处理
    let quota_manager_shutdown = quota_manager.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>()
    )
        .with_graceful_shutdown(shutdown_signal(quota_manager_shutdown))
        .await?;

    Ok(())
}

/// 优雅关闭信号处理
async fn shutdown_signal(quota_manager: Arc<QuotaManager>) {
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("无法监听 Ctrl+C 信号: {}", e);
        return;
    }
    
    println!("\n📦 正在保存配额数据...");
    
    if let Err(e) = quota_manager.save_all().await {
        eprintln!("❌ 保存失败: {}", e);
    } else {
        println!("✅ 数据已保存");
    }
}
