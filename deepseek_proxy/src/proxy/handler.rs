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
        }
    }

    // 2. 通过用户名获取Token许可（统一的生命周期和并发控制）
    let permit = state.login_limiter.acquire_permit_by_username(&claims.sub).await?;

    // 3. 强制设置为流式
    request.stream = true;

    // 记录聊天请求（获取模型和消息数量）
    let model = request.model.clone();
    let message_count = request.messages.len();
    
    // 4. 转发到 DeepSeek API
    let byte_stream = state.deepseek_client.chat_stream(request).await?;

    // 5. 上游请求成功，现在扣费
    state.quota_manager.increment_quota(&claims.sub).await?;

    // 记录聊天请求成功
    state.activity_logger.log_chat_request(&claims.sub, &model, message_count, None).await;
    tracing::info!("用户 {} 发起聊天请求: 模型={}, 消息数={}", claims.sub, model, message_count);

    // 6. 用 PermitGuardedStream 包装流，确保 permit 在整个流的生命周期内被持有
    let guarded_stream = crate::proxy::PermitGuardedStream::new(byte_stream, permit);
    let stream_body = Body::from_stream(guarded_stream);

    // 7. 构建 SSE 响应头
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
