use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("认证失败: {0}")]
    Unauthorized(String),

    #[error("资源不存在: {0}")]
    NotFound(String),

    #[error("配额已耗尽，需要付费")]
    PaymentRequired {
        used: u32,
        limit: u32,
        reset_at: String,
    },

    #[error("排队超时")]
    QueueTimeout,

    #[error("队列已满")]
    TooManyRequests,

    #[error("GLM API 超时")]
    GatewayTimeout,

    #[error("GLM API 错误: {0}")]
    GlmError(String),

    #[error("内部错误: {0}")]
    InternalError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            AppError::PaymentRequired { used, limit, reset_at } => {
                let body = Json(json!({
                    "error": "quota_exceeded",
                    "message": "月度配额已耗尽，请升级套餐或等待下月重置",
                    "details": {
                        "used": used,
                        "limit": limit,
                        "reset_at": reset_at
                    },
                    "upgrade_url": "https://your-site.com/upgrade"
                }));
                return (StatusCode::PAYMENT_REQUIRED, body).into_response();
            }
            AppError::QueueTimeout => (
                StatusCode::REQUEST_TIMEOUT,
                "queue_timeout",
                "请求排队超时,请等待 2-3 秒后重试".to_string(),
            ),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "queue_full",
                "服务繁忙,请等待 3-5 秒后重试".to_string(),
            ),
            AppError::GatewayTimeout => (
                StatusCode::GATEWAY_TIMEOUT,
                "glm_timeout",
                "GLM 服务响应超时,请等待 5-10 秒后重试".to_string(),
            ),
            AppError::GlmError(msg) => (StatusCode::BAD_GATEWAY, "glm_error", msg),
            AppError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg),
        };

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

// 兼容 anyhow::Error
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::InternalError(err.to_string())
    }
}
