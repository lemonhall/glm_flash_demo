mod auth;
mod config;
mod error;
mod deepseek;
mod proxy;

use auth::{login, auth_middleware, JwtService};
use axum::{
    middleware,
    routing::post,
    Router,
};
use config::Config;
use deepseek::DeepSeekClient;
use proxy::{proxy_chat, RateLimiter};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// 统一的应用状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub jwt_service: Arc<JwtService>,
    pub deepseek_client: Arc<DeepSeekClient>,
    pub rate_limiter: Arc<RateLimiter>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "glm_proxy=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 加载配置
    let config = Config::load()?;
    tracing::info!("配置加载成功");
    tracing::info!("服务器地址: {}:{}", config.server.host, config.server.port);
    tracing::info!("DeepSeek API: {}", config.deepseek.base_url);
    tracing::info!("限流: {} req/s", config.rate_limit.requests_per_second);
    tracing::info!("队列容量: {}", config.rate_limit.queue_capacity);

    // 初始化组件
    let jwt_service = Arc::new(JwtService::new(
        config.auth.jwt_secret.clone(),
        config.auth.token_ttl_seconds,
    ));

    let deepseek_client = Arc::new(DeepSeekClient::new(
        config.deepseek.api_key.clone(),
        config.deepseek.base_url.clone(),
        config.deepseek.timeout_seconds,
    ));

    let rate_limiter = Arc::new(RateLimiter::new(
        config.rate_limit.requests_per_second,
        config.rate_limit.queue_capacity,
        config.rate_limit.queue_timeout_seconds,
    ));

    let config = Arc::new(config);

    // 创建统一的应用状态
    let app_state = AppState {
        config: config.clone(),
        jwt_service,
        deepseek_client,
        rate_limiter,
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
    
    // 合并路由
    let app = public_routes
        .merge(protected_routes)
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());

    // 启动服务器
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    tracing::info!("🚀 DeepSeek 代理服务启动成功: http://{}", addr);
    tracing::info!("📝 登录接口: POST http://{}/auth/login", addr);
    tracing::info!("🔄 代理接口: POST http://{}/chat/completions", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
