use crate::error::AppError;
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use super::jwt::JwtService;

/// Token 验证中间件
pub async fn auth_middleware(
    State(jwt_service): State<Arc<JwtService>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    // 提取 Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("缺少 Authorization header".to_string()))?;

    // 提取 Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Authorization 格式错误".to_string()))?;

    // 验证 token
    let claims = jwt_service
        .validate_token(token)
        .map_err(|e| AppError::Unauthorized(format!("Token 无效: {}", e)))?;

    // 将用户信息存入 request extensions
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}
