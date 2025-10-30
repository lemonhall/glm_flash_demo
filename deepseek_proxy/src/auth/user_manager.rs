use crate::config::User;
use crate::error::AppError;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;

/// 用户管理器 - 管理内存中的用户状态并持久化到配置文件
#[derive(Clone)]
pub struct UserManager {
    /// 内存中的用户列表（可修改）
    users: Arc<RwLock<Vec<User>>>,
    /// 配置文件路径
    config_path: PathBuf,
}

impl UserManager {
    /// 创建用户管理器
    pub fn new(initial_users: Vec<User>, config_path: PathBuf) -> Self {
        Self {
            users: Arc::new(RwLock::new(initial_users)),
            config_path,
        }
    }

    /// 查找用户（用于登录验证）
    pub async fn find_user(&self, username: &str, password: &str) -> Option<User> {
        let users = self.users.read().await;
        users.iter()
            .find(|u| u.username == username && u.password == password)
            .cloned()
    }

    /// 设置用户的 is_active 状态
    pub async fn set_user_active(&self, username: &str, is_active: bool) -> Result<(), AppError> {
        // 1. 更新内存状态
        {
            let mut users = self.users.write().await;
            let user = users.iter_mut()
                .find(|u| u.username == username)
                .ok_or_else(|| AppError::NotFound(format!("用户 {} 不存在", username)))?;

            user.is_active = is_active;
            tracing::info!("用户 {} 的 is_active 状态已更新为: {}", username, is_active);
        }

        // 2. 持久化到配置文件
        self.save_to_config().await?;

        Ok(())
    }

    /// 获取用户信息
    pub async fn get_user(&self, username: &str) -> Option<User> {
        let users = self.users.read().await;
        users.iter()
            .find(|u| u.username == username)
            .cloned()
    }

    /// 获取所有用户（不含密码）
    pub async fn list_users(&self) -> Vec<UserInfo> {
        let users = self.users.read().await;
        users.iter()
            .map(|u| UserInfo {
                username: u.username.clone(),
                quota_tier: u.quota_tier.clone(),
                is_active: u.is_active,
            })
            .collect()
    }

    /// 保存到配置文件
    async fn save_to_config(&self) -> Result<(), AppError> {
        let users = self.users.read().await;

        // 读取现有配置文件
        let config_content = tokio::fs::read_to_string(&self.config_path)
            .await
            .map_err(|e| AppError::InternalError(format!("读取配置文件失败: {}", e)))?;

        // 解析为 toml 值
        let mut config_toml: toml::Value = toml::from_str(&config_content)
            .map_err(|e| AppError::InternalError(format!("解析配置文件失败: {}", e)))?;

        // 更新 auth.users 部分
        if let Some(auth) = config_toml.get_mut("auth") {
            if let Some(auth_table) = auth.as_table_mut() {
                let users_array: Vec<toml::Value> = users.iter()
                    .map(|u| {
                        let mut user_table = toml::map::Map::new();
                        user_table.insert("username".to_string(), toml::Value::String(u.username.clone()));
                        user_table.insert("password".to_string(), toml::Value::String(u.password.clone()));
                        user_table.insert("quota_tier".to_string(), toml::Value::String(u.quota_tier.clone()));
                        user_table.insert("is_active".to_string(), toml::Value::Boolean(u.is_active));
                        toml::Value::Table(user_table)
                    })
                    .collect();

                auth_table.insert("users".to_string(), toml::Value::Array(users_array));
            }
        }

        // 写回配置文件
        let new_config_content = toml::to_string_pretty(&config_toml)
            .map_err(|e| AppError::InternalError(format!("序列化配置失败: {}", e)))?;

        tokio::fs::write(&self.config_path, new_config_content)
            .await
            .map_err(|e| AppError::InternalError(format!("写入配置文件失败: {}", e)))?;

        tracing::info!("配置文件已更新: {:?}", self.config_path);
        Ok(())
    }
}

/// 用户信息（不含密码）
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserInfo {
    pub username: String,
    pub quota_tier: String,
    pub is_active: bool,
}
