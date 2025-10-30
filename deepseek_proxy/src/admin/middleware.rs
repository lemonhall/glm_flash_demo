use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;

/// 中间件：只允许 localhost 访问
pub async fn localhost_only(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    // 检查是否是 localhost
    let is_localhost = addr.ip().is_loopback();

    if !is_localhost {
        tracing::warn!("拒绝非 localhost 的管理请求，来源: {}", addr);
        return Err((
            StatusCode::FORBIDDEN,
            "Admin API only accessible from localhost",
        )
            .into_response());
    }

    tracing::debug!("允许来自 localhost 的管理请求: {}", addr);
    Ok(next.run(request).await)
}
