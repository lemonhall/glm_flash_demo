use crate::{config::Config, error::AppError};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::jwt::JwtService;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: u64,
}

pub async fn login(
    State(config): State<Arc<Config>>,
    State(jwt_service): State<Arc<JwtService>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // 验证用户名密码
    let user = config
        .auth
        .users
        .iter()
        .find(|u| u.username == req.username && u.password == req.password)
        .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;

    // 生成 token
    let token = jwt_service
        .generate_token(&user.username)
        .map_err(|e| AppError::Internal(format!("生成 token 失败: {}", e)))?;

    Ok(Json(LoginResponse {
        token,
        expires_in: config.auth.token_ttl_seconds,
    }))
}
