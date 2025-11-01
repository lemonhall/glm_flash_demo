use crate::config::User;
use crate::error::AppError;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;
use std::collections::HashMap;

/// 用户管理器 - 管理内存中的用户状态并持久化到独立文件
#[derive(Clone)]
pub struct UserManager {
    /// 内存中的用户映射（username -> User）
    users: Arc<RwLock<HashMap<String, User>>>,
    /// 用户文件存储目录
    users_dir: PathBuf,
}

impl UserManager {
    /// 创建用户管理器
    ///
    /// 初始化逻辑：
    /// 1. 如果 users_dir 为空，从 initial_users 导入
    /// 2. 如果 users_dir 有文件，从文件加载（忽略 initial_users）
    pub async fn new(users_dir: PathBuf, initial_users: Vec<User>) -> Result<Self, AppError> {
        // 确保目录存在
        tokio::fs::create_dir_all(&users_dir)
            .await
            .map_err(|e| AppError::InternalError(format!("创建用户目录失败: {}", e)))?;

        let manager = Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            users_dir,
        };

        // 加载现有用户文件
        let loaded_count = manager.load_all_users().await?;

        if loaded_count == 0 {
            // 目录为空，从 initial_users 导入
            tracing::info!("用户目录为空，从配置文件导入 {} 个用户", initial_users.len());
            for user in initial_users {
                manager.save_user(&user).await?;
            }
        } else {
            tracing::info!("从文件加载了 {} 个用户", loaded_count);
        }

        Ok(manager)
    }

    /// 从目录加载所有用户文件
    async fn load_all_users(&self) -> Result<usize, AppError> {
        let mut users = self.users.write().await;
        let mut count = 0;

        let mut entries = tokio::fs::read_dir(&self.users_dir)
            .await
            .map_err(|e| AppError::InternalError(format!("读取用户目录失败: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| AppError::InternalError(format!("读取目录条目失败: {}", e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                match Self::load_user_from_file(&path).await {
                    Ok(user) => {
                        users.insert(user.username.clone(), user);
                        count += 1;
                    }
                    Err(e) => {
                        tracing::warn!("加载用户文件失败 {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(count)
    }

    /// 从文件加载单个用户
    async fn load_user_from_file(path: &PathBuf) -> Result<User, AppError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AppError::InternalError(format!("读取用户文件失败: {}", e)))?;

        let user: User = toml::from_str(&content)
            .map_err(|e| AppError::InternalError(format!("解析用户文件失败: {}", e)))?;

        Ok(user)
    }

    /// 保存用户到文件
    async fn save_user(&self, user: &User) -> Result<(), AppError> {
        let file_path = self.users_dir.join(format!("{}.toml", user.username));

        let content = toml::to_string_pretty(user)
            .map_err(|e| AppError::InternalError(format!("序列化用户失败: {}", e)))?;

        tokio::fs::write(&file_path, content)
            .await
            .map_err(|e| AppError::InternalError(format!("写入用户文件失败: {}", e)))?;

        // 同时更新内存
        let mut users = self.users.write().await;
        users.insert(user.username.clone(), user.clone());

        tracing::debug!("用户文件已保存: {:?}", file_path);
        Ok(())
    }

    /// 查找用户（用于登录验证）
    pub async fn find_user(&self, username: &str, password: &str) -> Option<User> {
        let users = self.users.read().await;
        users.get(username)
            .filter(|u| u.password == password)
            .cloned()
    }

    /// 设置用户的 is_active 状态
    pub async fn set_user_active(&self, username: &str, is_active: bool) -> Result<(), AppError> {
        let users = self.users.read().await;
        let mut user = users.get(username)
            .ok_or_else(|| AppError::NotFound(format!("用户 {} 不存在", username)))?
            .clone();
        drop(users);

        // 更新状态和时间戳
        user.is_active = is_active;
        user.updated_at = Some(crate::utils::now_beijing_rfc3339());

        // 保存到文件（会同时更新内存）
        self.save_user(&user).await?;

        tracing::info!("用户 {} 的 is_active 状态已更新为: {}", username, is_active);
        Ok(())
    }

    /// 获取用户信息
    pub async fn get_user(&self, username: &str) -> Option<User> {
        let users = self.users.read().await;
        users.get(username).cloned()
    }

    /// 获取所有用户（不含密码）
    pub async fn list_users(&self) -> Vec<UserInfo> {
        let users = self.users.read().await;
        users.values()
            .map(|u| UserInfo {
                username: u.username.clone(),
                quota_tier: u.quota_tier.clone(),
                is_active: u.is_active,
            })
            .collect()
    }

    /// 校验用户名是否合法
    /// 
    /// 规则：
    /// - 长度 3-32 字符
    /// - 只允许字母、数字、下划线和连字符
    /// - 必须以字母或数字开头
    /// - 不能包含路径分隔符或特殊字符
    fn validate_username(username: &str) -> Result<(), AppError> {
        // 检查长度
        if username.len() < 3 || username.len() > 32 {
            return Err(AppError::BadRequest(
                "用户名长度必须在 3-32 个字符之间".to_string()
            ));
        }

        // 检查是否以字母或数字开头
        if let Some(first_char) = username.chars().next() {
            if !first_char.is_alphanumeric() {
                return Err(AppError::BadRequest(
                    "用户名必须以字母或数字开头".to_string()
                ));
            }
        }

        // 检查字符是否合法（只允许字母、数字、下划线、连字符）
        for ch in username.chars() {
            if !ch.is_alphanumeric() && ch != '_' && ch != '-' {
                return Err(AppError::BadRequest(
                    format!("用户名包含非法字符: '{}'. 只允许字母、数字、下划线和连字符", ch)
                ));
            }
        }

        // 检查是否包含路径相关的危险字符串
        let dangerous_patterns = [".", "..", "/", "\\", "\0"];
        for pattern in dangerous_patterns {
            if username.contains(pattern) {
                return Err(AppError::BadRequest(
                    format!("用户名不能包含危险字符或模式: '{}'", pattern)
                ));
            }
        }

        Ok(())
    }

    /// 创建新用户
    pub async fn create_user(&self, username: String, password: String, quota_tier: String) -> Result<(), AppError> {
        // 校验用户名合法性
        Self::validate_username(&username)?;

        // 检查用户是否已存在
        {
            let users = self.users.read().await;
            if users.contains_key(&username) {
                return Err(AppError::InternalError(format!("用户 {} 已存在", username)));
            }
        }

        let now = crate::utils::now_beijing_rfc3339();
        let user = User {
            username: username.clone(),
            password,
            quota_tier,
            is_active: true,
            created_at: Some(now.clone()),
            updated_at: Some(now),
        };

        self.save_user(&user).await?;
        tracing::info!("用户 {} 已创建", username);
        Ok(())
    }

    // 注意：不提供物理删除功能，只能通过 set_user_active(username, false) 进行逻辑删除
}

/// 用户信息（不含密码）
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserInfo {
    pub username: String,
    pub quota_tier: String,
    pub is_active: bool,
}
