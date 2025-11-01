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
    // 验证用户名密码（从内存中的用户管理器获取）
    let user = state
        .user_manager
        .find_user(&req.username, &req.password)
        .await
        .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;

    // 检查账户是否已激活
    if !user.is_active {
        tracing::warn!("用户 {} 尝试登录，但账户已被停用", user.username);
        return Err(AppError::Unauthorized("账户已被停用".to_string()));
    }

    // 使用登录限流器：在有效期内返回同一个 token（最多 60 秒）
    let token = state.login_limiter
        .get_or_generate(&user.username, || {
            state
                .jwt_service
                .generate_token(&user.username)
                .map_err(|e| AppError::InternalError(format!("Token生成失败: {}", e)))
        })
        .await?;

    Ok(Json(LoginResponse {
        token,
        expires_in: state.jwt_service.get_ttl_seconds(),  // 返回实际的 TTL（已被限制为最多 60 秒）
    }))
}
