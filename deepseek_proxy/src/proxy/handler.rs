use crate::{
    auth::Claims,
    error::AppError,
    deepseek::ChatRequest,
    quota::QuotaStatus,
    AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};

/// 代理聊天请求到 DeepSeek API
pub async fn proxy_chat(
    State(state): State<AppState>,
    Extension(token): Extension<String>,
    Extension(claims): Extension<Claims>,
    Json(mut request): Json<ChatRequest>,
) -> Result<Response, AppError> {
    // 1. 检查配额
    let quota_status = state.quota_manager
        .check_and_increment(&claims.sub)
        .await?;
    
    match quota_status {
        QuotaStatus::Exceeded { used, limit, reset_at } => {
            tracing::warn!("用户 {} 配额已耗尽: {}/{}", claims.sub, used, limit);
            return Err(AppError::PaymentRequired {
                used,
                limit,
                reset_at: reset_at.to_rfc3339(),
            });
        }
        QuotaStatus::Ok { used, remaining, .. } => {
            tracing::debug!("用户 {} 配额检查通过: {}次已用, {}次剩余", claims.sub, used, remaining);
        }
    }

    // 2. 获取该 token 的限流许可（同一 token 同时只允耸1个请求）
    let _permit = state.token_limiter.acquire(&token).await?;

    // 3. 强制设置为流式
    request.stream = true;

    // 4. 转发到 DeepSeek API
    let byte_stream = state.deepseek_client.chat_stream(request).await?;

    // 5. 直接透传
    let stream_body = Body::from_stream(byte_stream);

    // 6. 构建 SSE 响应头
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    headers.insert(header::CONNECTION, "keep-alive".parse().unwrap());

    Ok((StatusCode::OK, headers, stream_body).into_response())
}
