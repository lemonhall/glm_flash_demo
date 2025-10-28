use crate::{
    error::AppError,
    deepseek::ChatRequest,
    AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

/// 代理聊天请求到 DeepSeek API
pub async fn proxy_chat(
    State(state): State<AppState>,
    Json(mut request): Json<ChatRequest>,
) -> Result<Response, AppError> {
    // 1. 获取限流许可 (带超时和队列满检查)
    let _permit = state.rate_limiter.acquire().await?;

    // 2. 强制设置为流式
    request.stream = true;

    // 3. 转发到 DeepSeek API
    let byte_stream = state.deepseek_client.chat_stream(request).await?;

    // 4. 直接透传
    let stream_body = Body::from_stream(byte_stream);

    // 5. 构建 SSE 响应头
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    headers.insert(header::CONNECTION, "keep-alive".parse().unwrap());

    Ok((StatusCode::OK, headers, stream_body).into_response())
}
