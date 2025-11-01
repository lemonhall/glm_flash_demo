use crate::{
    auth::Claims,
    error::AppError,
    deepseek::ChatRequest,
    quota::QuotaStatus,
    AppState,
};

// HTTP 头部常量
const CONTENT_TYPE_SSE: &str = "text/event-stream";
const CACHE_CONTROL_NO_CACHE: &str = "no-cache";
const CONNECTION_KEEP_ALIVE: &str = "keep-alive";
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use futures::Stream;
use bytes::Bytes;
use std::pin::Pin;
use std::task::{Context, Poll};

/// 简单估算输入 tokens: 按空白分词 + 中文字符单字
fn estimate_input_tokens(messages: &[crate::deepseek::Message]) -> u32 {
    let mut count = 0u32;
    for m in messages {
        let text = m.content.as_str();
        // 中文单字
        count += text.chars().filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c)).count() as u32;
        // 英文/数字等按空白分词
        for part in text.split_whitespace() {
            if !part.is_empty() { count += 1; }
        }
    }
    count
}

/// 统计输出 token 的流包装器：累计字节数，在 Drop 时估算 token 数 (粗略: 字节/4)
struct CountingStream<S> {
    inner: S,
    bytes_acc: usize,
    recorded: bool,
    username: String,
    real_output_recorded: bool,
}

impl<S> CountingStream<S> {
    fn new(inner: S, username: String) -> Self { Self { inner, bytes_acc: 0, recorded: false, username, real_output_recorded: false } }
}

impl<S> Stream for CountingStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.bytes_acc += chunk.len();
                // 尝试解析 usage
                if !self.real_output_recorded {
                    if let Ok(text) = std::str::from_utf8(&chunk) {
                        for line in text.lines() {
                            let line = line.trim();
                            if line.starts_with("data:") {
                                let json_part = line.trim_start_matches("data:").trim();
                                if json_part == "[DONE]" { continue; }
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_part) {
                                    if let Some(usage) = v.get("usage") {
                                        let completion = usage.get("completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                        let prompt = usage.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                        let cache_hit = usage.get("prompt_cache_hit_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                        let cache_miss = usage.get("prompt_cache_miss_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                        let reasoning = usage.get("completion_tokens_details").and_then(|d| d.get("reasoning_tokens")).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                                        // 记录输出与输入
                                        crate::metrics::METRICS.record_output_tokens(completion);
                                        crate::metrics::METRICS.record_input_tokens(prompt); // 修正输入 gauge
                                        crate::metrics::METRICS.record_prompt_cache_hit_tokens(cache_hit);
                                        crate::metrics::METRICS.record_prompt_cache_miss_tokens(cache_miss);
                                        tracing::debug!(user=%self.username, prompt_tokens=prompt, completion_tokens=completion, cache_hit=cache_hit, cache_miss=cache_miss, reasoning_tokens=reasoning, "使用真实 usage 字段记录 token 与缓存命中");
                                        self.real_output_recorded = true;
                                    }
                                }
                            }
                        }
                    }
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            other => other,
        }
    }
}

impl<S> Drop for CountingStream<S> {
    fn drop(&mut self) {
        // 如果已经通过 usage 记录过真实 completion，则不再估算
        if !self.recorded && !self.real_output_recorded {
            let bytes = self.bytes_acc as u32;
            // 粗略估算：假设平均 4 字节一个 token
            let tokens = bytes / 4;
            crate::metrics::METRICS.record_output_tokens(tokens);
            tracing::debug!(user = %self.username, bytes = bytes, tokens = tokens, "输出 token 估算");
            self.recorded = true;
        }
    }
}

/// 代理聊天请求到 DeepSeek API
pub async fn proxy_chat(
    State(state): State<AppState>,
    Extension(_token): Extension<String>,
    Extension(claims): Extension<Claims>,
    Json(mut request): Json<ChatRequest>,
) -> Result<Response, AppError> {
    // 0. 全局速率限制检查（最优先，防止 DoS）
    if let Err(wait_time) = state.global_rate_limiter.acquire().await {
        tracing::warn!("全局速率限制：拒绝请求，建议等待 {:.2} 秒", wait_time);
        crate::metrics::METRICS.rate_limit_rejections.inc();
        return Err(AppError::TooManyRequests);
    }

    // 1. 检查配额（不扣费）
    let quota_status = state.quota_manager
        .check_quota(&claims.sub)
        .await?;

    match quota_status {
        QuotaStatus::Exceeded { used, limit, reset_at } => {
            tracing::warn!("用户 {} 配额已耗尽: {}/{}", claims.sub, used, limit);
            // 记录配额耗尽
            state.activity_logger.log_quota_exceeded(&claims.sub, used, limit).await;
            crate::metrics::METRICS.quota_status.with_label_values(&["exceeded"]).inc();
            return Err(AppError::PaymentRequired {
                used,
                limit,
                reset_at: reset_at.to_rfc3339(),
            });
        }
        QuotaStatus::Ok { used, remaining, .. } => {
            tracing::debug!("用户 {} 配额检查通过: {}次已用, {}次剩余", claims.sub, used, remaining);
            // 记录配额检查
            state.activity_logger.log_quota_check(&claims.sub, used, remaining).await;
            crate::metrics::METRICS.quota_status.with_label_values(&["ok"]).inc();
        }
    }

    // 2. 通过用户名获取Token许可（统一的生命周期和并发控制）
    let permit = state.login_limiter.acquire_permit_by_username(&claims.sub).await?;

    // 3. 强制设置为流式
    request.stream = true;

    // 记录聊天请求（获取模型和消息数量）
    let model = request.model.clone();
    let message_count = request.messages.len();
    
    // 4. 估算输入 token
    let input_tokens = estimate_input_tokens(&request.messages);
    crate::metrics::METRICS.record_input_tokens(input_tokens);
    tracing::debug!(user = %claims.sub, tokens = input_tokens, "输入 token 估算");

    // 5. 转发到 DeepSeek API
    let byte_stream = state.deepseek_client.chat_stream(request).await?;

    // 6. 上游请求成功，现在扣费
    state.quota_manager.increment_quota(&claims.sub).await?;

    // 记录聊天请求成功
    state.activity_logger.log_chat_request(&claims.sub, &model, message_count, None).await;
    tracing::info!("用户 {} 发起聊天请求: 模型={}, 消息数={}", claims.sub, model, message_count);
    crate::metrics::METRICS.chat_requests.with_label_values(&["success"]).inc();

    // 7. 用 PermitGuardedStream 包装流，确保 permit 在整个流的生命周期内被持有
    let guarded_stream = crate::proxy::PermitGuardedStream::new(byte_stream, permit);
    // 再包一层 CountingStream 做输出 token 统计
    let counting_stream = CountingStream::new(guarded_stream, claims.sub.clone());
    let stream_body = Body::from_stream(counting_stream);

    // 8. 构建 SSE 响应头
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE, 
        CONTENT_TYPE_SSE.parse().map_err(|_| AppError::InternalError("无效的Content-Type头".to_string()))?
    );
    headers.insert(
        header::CACHE_CONTROL, 
        CACHE_CONTROL_NO_CACHE.parse().map_err(|_| AppError::InternalError("无效的Cache-Control头".to_string()))?
    );
    headers.insert(
        header::CONNECTION, 
        CONNECTION_KEEP_ALIVE.parse().map_err(|_| AppError::InternalError("无效的Connection头".to_string()))?
    );

    Ok((StatusCode::OK, headers, stream_body).into_response())
}
