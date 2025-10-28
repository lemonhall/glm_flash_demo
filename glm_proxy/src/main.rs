mod auth;
mod config;
mod error;
mod glm;
mod proxy;

use auth::{login, auth_middleware, JwtService};
use axum::{
    middleware,
    routing::post,
    Router,
};
use config::Config;
use glm::GlmClient;
use proxy::{proxy_chat, RateLimiter};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
    tracing::info!("GLM API: {}", config.glm.base_url);
    tracing::info!("限流: {} req/s", config.rate_limit.requests_per_second);
    tracing::info!("队列容量: {}", config.rate_limit.queue_capacity);

    // 初始化组件
    let jwt_service = Arc::new(JwtService::new(
        config.auth.jwt_secret.clone(),
        config.auth.token_ttl_seconds,
    ));

    let glm_client = Arc::new(GlmClient::new(
        config.glm.api_key.clone(),
        config.glm.base_url.clone(),
        config.glm.timeout_seconds,
    ));

    let rate_limiter = Arc::new(RateLimiter::new(
        config.rate_limit.requests_per_second,
        config.rate_limit.queue_capacity,
        config.rate_limit.queue_timeout_seconds,
    ));

    let config = Arc::new(config);

    // 构建路由
    let app = Router::new()
        // 认证接口 (无需 token)
        .route("/auth/login", post(login))
        .with_state(config.clone())
        .with_state(jwt_service.clone())
        
        // 代理接口 (需要 token)
        .route("/chat/completions", post(proxy_chat))
        .layer(middleware::from_fn_with_state(
            jwt_service.clone(),
            auth_middleware,
        ))
        .with_state(glm_client)
        .with_state(rate_limiter)
        
        // 添加日志追踪
        .layer(TraceLayer::new_for_http());

    // 启动服务器
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    tracing::info!("🚀 GLM 代理服务启动成功: http://{}", addr);
    tracing::info!("📝 登录接口: POST http://{}/auth/login", addr);
    tracing::info!("🔄 代理接口: POST http://{}/chat/completions", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
