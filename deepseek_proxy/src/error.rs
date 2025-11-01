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

    #[error("请求参数错误: {0}")]
    BadRequest(String),

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
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
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
// 注意：anyhow::Error 会被转换为 InternalError
// 建议在业务代码中尽量使用具体的 AppError 变体以便更好地分类错误
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        // 记录详细的错误链，便于调试
        tracing::error!(
            error = %err,
            backtrace = ?err.backtrace(),
            "anyhow::Error 被转换为 InternalError"
        );
        
        // 将完整的错误链转换为字符串
        let error_chain = err
            .chain()
            .enumerate()
            .map(|(i, e)| format!("  [{}] {}", i, e))
            .collect::<Vec<_>>()
            .join("\n");
        
        AppError::InternalError(format!(
            "内部错误:\n{}",
            error_chain
        ))
    }
}

// 为常见的标准库错误提供更具体的转换
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        tracing::error!(
            error = %err,
            kind = ?err.kind(),
            "IO 错误"
        );
        
        match err.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::NotFound(format!("文件或资源不存在: {}", err))
            }
            std::io::ErrorKind::PermissionDenied => {
                AppError::InternalError(format!("权限不足: {}", err))
            }
            std::io::ErrorKind::TimedOut => {
                AppError::GatewayTimeout
            }
            _ => {
                AppError::InternalError(format!("IO 错误: {}", err))
            }
        }
    }
}

// 为 serde 错误提供更具体的转换
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        tracing::warn!(
            error = %err,
            line = err.line(),
            column = err.column(),
            "JSON 序列化/反序列化错误"
        );
        
        AppError::BadRequest(format!(
            "JSON 格式错误 (行 {}, 列 {}): {}",
            err.line(),
            err.column(),
            err
        ))
    }
}

impl AppError {
    /// 创建带上下文的内部错误
    /// 
    /// 使用示例：
    /// ```
    /// AppError::internal_with_context("配额保存失败", &err)
    /// ```
    pub fn internal_with_context(context: &str, err: &dyn std::fmt::Display) -> Self {
        tracing::error!(
            context = context,
            error = %err,
            "内部错误发生"
        );
        AppError::InternalError(format!("{}: {}", context, err))
    }
    
    /// 创建带错误码的内部错误（便于运维查询日志）
    /// 
    /// 使用示例：
    /// ```
    /// AppError::internal_with_code("CFG001", "配置文件加载失败")
    /// ```
    pub fn internal_with_code(code: &str, message: &str) -> Self {
        tracing::error!(
            error_code = code,
            message = message,
            "内部错误发生"
        );
        AppError::InternalError(format!("[{}] {}", code, message))
    }
    
    /// 从 anyhow::Error 创建带上下文的错误
    /// 
    /// 使用示例：
    /// ```
    /// AppError::from_anyhow_with_context("用户文件加载失败", err)
    /// ```
    pub fn from_anyhow_with_context(context: &str, err: anyhow::Error) -> Self {
        tracing::error!(
            context = context,
            error = %err,
            backtrace = ?err.backtrace(),
            "anyhow 错误发生"
        );
        
        let error_chain = err
            .chain()
            .enumerate()
            .map(|(i, e)| format!("  [{}] {}", i, e))
            .collect::<Vec<_>>()
            .join("\n");
        
        AppError::InternalError(format!(
            "{}:\n{}",
            context,
            error_chain
        ))
    }
}

