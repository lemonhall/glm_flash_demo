use crate::{error::AppError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

/// 设置用户激活状态的请求
#[derive(Debug, Deserialize)]
pub struct SetUserActiveRequest {
    pub is_active: bool,
}

/// 设置用户激活状态的响应
#[derive(Debug, Serialize)]
pub struct SetUserActiveResponse {
    pub username: String,
    pub is_active: bool,
    pub message: String,
}

/// 管理接口：设置用户的 is_active 状态
/// 只能从 localhost 访问（由中间件控制）
pub async fn set_user_active(
    State(state): State<AppState>,
    Path(username): Path<String>,
    Json(req): Json<SetUserActiveRequest>,
) -> Result<Json<SetUserActiveResponse>, AppError> {
    // 设置用户状态（会同时更新内存和配置文件）
    state.user_manager
        .set_user_active(&username, req.is_active)
        .await?;

    let message = if req.is_active {
        format!("用户 {} 已启用", username)
    } else {
        format!("用户 {} 已停用", username)
    };

    Ok(Json(SetUserActiveResponse {
        username,
        is_active: req.is_active,
        message,
    }))
}

/// 获取用户信息
#[derive(Debug, Serialize)]
pub struct GetUserResponse {
    pub username: String,
    pub quota_tier: String,
    pub is_active: bool,
}

/// 管理接口：获取用户信息
pub async fn get_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<GetUserResponse>, AppError> {
    let user = state.user_manager
        .get_user(&username)
        .await
        .ok_or_else(|| AppError::NotFound(format!("用户 {} 不存在", username)))?;

    Ok(Json(GetUserResponse {
        username: user.username,
        quota_tier: user.quota_tier,
        is_active: user.is_active,
    }))
}

/// 管理接口：列出所有用户
#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<crate::auth::UserInfo>,
}

pub async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<ListUsersResponse>, AppError> {
    let users = state.user_manager.list_users().await;

    Ok(Json(ListUsersResponse { users }))
}

/// 创建用户请求
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(default = "default_quota_tier")]
    pub quota_tier: String,
}

fn default_quota_tier() -> String {
    "basic".to_string()
}

/// 创建用户响应
#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub username: String,
    pub message: String,
}

/// 管理接口：创建新用户
pub async fn create_user(
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, AppError> {
    state.user_manager
        .create_user(req.username.clone(), req.password, req.quota_tier)
        .await?;

    Ok(Json(CreateUserResponse {
        username: req.username.clone(),
        message: format!("用户 {} 已创建", req.username),
    }))
}

// 注意：不提供物理删除功能
// 要"删除"用户，请使用 POST /admin/users/:username/active 并设置 is_active = false
