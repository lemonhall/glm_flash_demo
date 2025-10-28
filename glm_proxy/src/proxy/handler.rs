use crate::{
    error::AppError,
    glm::{ChatRequest, GlmClient},
};
use axum::{
    body::Body,
    extract::State,
    response::{sse::Event, IntoResponse, Response, Sse},
    Json,
};
use futures::stream::StreamExt;
use std::{sync::Arc, time::Instant};
use tokio::time::Duration;

use super::limiter::RateLimiter;

/// 代理聊天请求到 GLM API
pub async fn proxy_chat(
    State(glm_client): State<Arc<GlmClient>>,
    State(rate_limiter): State<Arc<RateLimiter>>,
    Json(mut request): Json<ChatRequest>,
) -> Result<Response, AppError> {
    // 1. 获取限流许可 (带超时和队列满检查)
    let _permit = rate_limiter.acquire().await?;

    // 2. 强制设置为流式
    request.stream = true;

    // 3. 转发到 GLM API
    let stream = glm_client.chat_stream(request).await?;

    // 4. 监控总超时 (20秒)
    let start_time = Instant::now();
    let timeout_duration = Duration::from_secs(20);

    // 5. 转换为 SSE 流并添加超时检查
    let sse_stream = stream.map(move |chunk_result| {
        // 检查总超时
        if start_time.elapsed() > timeout_duration {
            return Err(AppError::GatewayTimeout);
        }

        // 处理数据块
        match chunk_result {
            Ok(bytes) => {
                // 直接透传 GLM 返回的 SSE 数据
                String::from_utf8(bytes.to_vec())
                    .map(|text| Event::default().data(text))
                    .map_err(|e| AppError::Internal(format!("UTF-8 解码失败: {}", e)))
            }
            Err(e) => Err(AppError::GlmError(format!("GLM 流式响应错误: {}", e))),
        }
    });

    // 6. 返回 SSE 响应
    Ok(Sse::new(sse_stream).into_response())
}
