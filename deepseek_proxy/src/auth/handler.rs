use crate::{error::AppError, AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

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
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // 验证用户名密码
    let user = state
        .config
        .auth
        .users
        .iter()
        .find(|u| u.username == req.username && u.password == req.password)
        .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;

    // 生成 token
    let token = state
        .jwt_service
        .generate_token(&user.username)
        .map_err(|e| AppError::Internal(format!("生成 token 失败: {}", e)))?;

    Ok(Json(LoginResponse {
        token,
        expires_in: state.config.auth.token_ttl_seconds,
    }))
}
