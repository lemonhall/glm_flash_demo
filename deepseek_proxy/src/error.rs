use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

// ============================================================================
// 分层错误定义
// ============================================================================

/// 认证/授权相关错误
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("认证失败: {0}")]
    Unauthorized(String),
    
    #[error("Token 已过期")]
    TokenExpired,
    
    #[error("Token 无效")]
    InvalidToken,
    
    #[error("用户不存在")]
    UserNotFound,
    
    #[error("账户已被停用")]
    AccountDisabled,
    
    #[error("密码错误")]
    InvalidCredentials,
}

/// 配额相关错误
#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    #[error("配额已耗尽")]
    Exceeded {
        used: u32,
        limit: u32,
        reset_at: String,
    },
    
    #[error("配额文件读取失败: {0}")]
    FileReadError(String),
    
    #[error("配额文件写入失败: {0}")]
    FileWriteError(String),
    
    #[error("配额层级无效: {0}")]
    InvalidTier(String),
}

/// 上游服务（DeepSeek API）相关错误
#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("上游服务超时")]
    Timeout,
    
    #[error("上游服务返回错误 (状态码 {status}): {message}")]
    ApiError {
        status: u16,
        message: String,
    },
    
    #[error("上游服务网络错误: {0}")]
    NetworkError(String),
    
    #[error("上游服务响应格式错误: {0}")]
    InvalidResponse(String),
}

/// 系统/内部错误
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("内部错误: {0}")]
    Internal(String),
    
    #[error("配置错误: {0}")]
    Configuration(String),
    
    #[error("文件 IO 错误: {0}")]
    FileIo(String),
    
    #[error("JSON 序列化错误: {0}")]
    Serialization(String),
    
    #[error("数据库错误: {0}")]
    Database(String),
}

/// 统一的应用错误枚举（向后兼容）
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("认证错误: {0}")]
    Auth(#[from] AuthError),
    
    #[error("配额错误: {0}")]
    Quota(#[from] QuotaError),
    
    #[error("上游服务错误: {0}")]
    Upstream(#[from] UpstreamError),
    
    #[error("系统错误: {0}")]
    System(#[from] SystemError),
    
    // 保留常用的快捷变体以保持向后兼容
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
            // 分层错误处理
            AppError::Auth(auth_err) => match auth_err {
                AuthError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg),
                AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "token_expired", "Token 已过期，请重新登录".to_string()),
                AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token", "Token 无效".to_string()),
                AuthError::UserNotFound => (StatusCode::UNAUTHORIZED, "user_not_found", "用户不存在".to_string()),
                AuthError::AccountDisabled => (StatusCode::FORBIDDEN, "account_disabled", "账户已被停用".to_string()),
                AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials", "用户名或密码错误".to_string()),
            },
            
            AppError::Quota(quota_err) => match quota_err {
                QuotaError::Exceeded { used, limit, reset_at } => {
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
                },
                QuotaError::FileReadError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "quota_file_read_error", msg),
                QuotaError::FileWriteError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "quota_file_write_error", msg),
                QuotaError::InvalidTier(msg) => (StatusCode::BAD_REQUEST, "invalid_quota_tier", msg),
            },
            
            AppError::Upstream(upstream_err) => match upstream_err {
                UpstreamError::Timeout => (
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "上游服务响应超时，请等待 5-10 秒后重试".to_string(),
                ),
                UpstreamError::ApiError { status, message } => (
                    StatusCode::BAD_GATEWAY,
                    "upstream_api_error",
                    format!("上游服务返回错误 (状态码 {}): {}", status, message),
                ),
                UpstreamError::NetworkError(msg) => (
                    StatusCode::BAD_GATEWAY,
                    "upstream_network_error",
                    format!("上游服务网络错误: {}", msg),
                ),
                UpstreamError::InvalidResponse(msg) => (
                    StatusCode::BAD_GATEWAY,
                    "upstream_invalid_response",
                    format!("上游服务响应格式错误: {}", msg),
                ),
            },
            
            AppError::System(system_err) => match system_err {
                SystemError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg),
                SystemError::Configuration(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "configuration_error", msg),
                SystemError::FileIo(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "file_io_error", msg),
                SystemError::Serialization(msg) => (StatusCode::BAD_REQUEST, "serialization_error", msg),
                SystemError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error", msg),
            },
            
            // 向后兼容的快捷变体
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
                "请求排队超时，请等待 2-3 秒后重试".to_string(),
            ),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                "服务繁忙，请等待 3-5 秒后重试".to_string(),
            ),
            AppError::GatewayTimeout => (
                StatusCode::GATEWAY_TIMEOUT,
                "gateway_timeout",
                "上游服务响应超时，请等待 5-10 秒后重试".to_string(),
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

// ============================================================================
// 错误转换实现
// ============================================================================

// 兼容 anyhow::Error - 转换为 SystemError
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(
            error = %err,
            backtrace = ?err.backtrace(),
            "anyhow::Error 被转换为 SystemError"
        );
        
        let error_chain = err
            .chain()
            .enumerate()
            .map(|(i, e)| format!("  [{}] {}", i, e))
            .collect::<Vec<_>>()
            .join("\n");
        
        AppError::System(SystemError::Internal(format!(
            "内部错误:\n{}",
            error_chain
        )))
    }
}

// 标准库 IO 错误转换
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
                AppError::System(SystemError::FileIo(format!("权限不足: {}", err)))
            }
            std::io::ErrorKind::TimedOut => {
                AppError::GatewayTimeout
            }
            _ => {
                AppError::System(SystemError::FileIo(format!("IO 错误: {}", err)))
            }
        }
    }
}

// JSON 序列化错误转换
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        tracing::warn!(
            error = %err,
            line = err.line(),
            column = err.column(),
            "JSON 序列化/反序列化错误"
        );
        
        AppError::System(SystemError::Serialization(format!(
            "JSON 格式错误 (行 {}, 列 {}): {}",
            err.line(),
            err.column(),
            err
        )))
    }
}

// ============================================================================
// 便捷构造方法
// ============================================================================

impl AppError {
    /// 创建认证错误 - 用户不存在
    pub fn user_not_found() -> Self {
        AppError::Auth(AuthError::UserNotFound)
    }
    
    /// 创建认证错误 - 账户已停用
    pub fn account_disabled() -> Self {
        AppError::Auth(AuthError::AccountDisabled)
    }
    
    /// 创建认证错误 - 凭据无效
    pub fn invalid_credentials() -> Self {
        AppError::Auth(AuthError::InvalidCredentials)
    }
    
    /// 创建认证错误 - Token 过期
    pub fn token_expired() -> Self {
        AppError::Auth(AuthError::TokenExpired)
    }
    
    /// 创建配额错误 - 配额已耗尽
    pub fn quota_exceeded(used: u32, limit: u32, reset_at: String) -> Self {
        AppError::Quota(QuotaError::Exceeded { used, limit, reset_at })
    }
    
    /// 创建上游错误 - API 错误
    pub fn upstream_api_error(status: u16, message: String) -> Self {
        AppError::Upstream(UpstreamError::ApiError { status, message })
    }
    
    /// 创建上游错误 - 超时
    pub fn upstream_timeout() -> Self {
        AppError::Upstream(UpstreamError::Timeout)
    }
    
    /// 创建系统错误 - 配置错误
    pub fn configuration_error(msg: impl Into<String>) -> Self {
        AppError::System(SystemError::Configuration(msg.into()))
    }
    
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
        AppError::System(SystemError::Internal(format!("{}: {}", context, err)))
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
        AppError::System(SystemError::Internal(format!("[{}] {}", code, message)))
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
        
        AppError::System(SystemError::Internal(format!(
            "{}:\n{}",
            context,
            error_chain
        )))
    }
}
