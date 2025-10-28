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

    // 使用登录限流器：1 分钟内返回同一个 token
    let token = state.login_limiter
        .get_or_generate(&user.username, || {
            state
                .jwt_service
                .generate_token(&user.username)
                .expect("Failed to generate token")
        })
        .await;

    Ok(Json(LoginResponse {
        token,
        expires_in: state.config.auth.token_ttl_seconds,
    }))
}
