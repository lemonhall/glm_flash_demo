use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

/// 用户行为类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAction {
    /// 登录
    Login,
    /// 登出
    Logout,
    /// 聊天请求
    ChatRequest {
        model: String,
        message_count: usize,
        tokens_estimated: Option<u32>,
    },
    /// 配额检查
    QuotaCheck {
        used: u32,
        remaining: u32,
    },
    /// 配额耗尽
    QuotaExceeded {
        used: u32,
        limit: u32,
    },
    /// 速率限制触发
    RateLimited,
    /// 账户被停用
    AccountDisabled,
    /// 错误
    Error {
        error_type: String,
        message: String,
    },
}

/// 用户行为日志记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActivityLog {
    /// 时间戳 (RFC3339)
    pub timestamp: String,
    /// 用户名
    pub username: String,
    /// 行为类型
    pub action: UserAction,
    /// IP 地址（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// 请求 ID（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// 额外信息（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// 用户行为日志记录器
#[derive(Clone)]
pub struct UserActivityLogger {
    base_dir: PathBuf,
    max_file_size: u64,
    file_handles: Arc<Mutex<HashMap<String, (tokio::fs::File, u64)>>>, // username -> (file, current_size)
}

impl UserActivityLogger {
    /// 创建新的用户行为日志记录器
    /// 
    /// 特性：
    /// - 每个用户独立文件夹：logs/users/{username}/
    /// - 按日期自动滚动：{username}.2025-11-01.log
    /// - 按大小自动滚动：单个文件最大 5MB
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            max_file_size: 5 * 1024 * 1024, // 5 MB
            file_handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 记录用户行为
    pub async fn log(&self, log: UserActivityLog) {
        if let Err(e) = self.write_log(&log).await {
            tracing::error!(
                username = %log.username,
                action = ?log.action,
                error = %e,
                "写入用户行为日志失败"
            );
        }
    }

    /// 写入日志到文件
    async fn write_log(&self, log: &UserActivityLog) -> anyhow::Result<()> {
        let username = sanitize_username(&log.username);
        
        // 用户日志目录：logs/users/{username}/
        let user_log_dir = self.base_dir.join(&username);
        tokio::fs::create_dir_all(&user_log_dir).await?;

        // 当前日期
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        
        // 日志文件名：{username}.2025-11-01.log
        let log_filename = format!("{}.{}.log", username, today);
        let log_file_path = user_log_dir.join(&log_filename);

        // 序列化为 JSON（一行一条记录）
        let mut json_line = serde_json::to_string(log)?;
        json_line.push('\n');
        let line_size = json_line.len() as u64;

        // 检查文件大小，如果超过限制则重命名旧文件
        let mut handles = self.file_handles.lock().await;
        let cache_key = format!("{}:{}", username, today);
        
        // 检查是否需要滚动文件
        if let Ok(metadata) = tokio::fs::metadata(&log_file_path).await {
            if metadata.len() >= self.max_file_size {
                // 文件太大，重命名并创建新文件
                let timestamp = chrono::Local::now().format("%H%M%S").to_string();
                let archived_name = format!("{}.{}.{}.log", username, today, timestamp);
                let archived_path = user_log_dir.join(&archived_name);
                
                // 关闭旧的文件句柄
                handles.remove(&cache_key);
                
                // 重命名
                tokio::fs::rename(&log_file_path, &archived_path).await?;
                
                tracing::info!(
                    "用户日志文件滚动: {} -> {}",
                    log_file_path.display(),
                    archived_path.display()
                );
                
                // 异步清理旧文件
                let user_log_dir_clone = user_log_dir.clone();
                let username_clone = username.clone();
                tokio::spawn(async move {
                    if let Err(e) = cleanup_old_logs(&user_log_dir_clone, &username_clone).await {
                        tracing::warn!("清理旧日志文件失败: {}", e);
                    }
                });
            }
        }

        // 追加写入文件
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .await?;

        file.write_all(json_line.as_bytes()).await?;
        file.flush().await?;

        // 更新缓存
        handles.insert(cache_key, (file, line_size));

        Ok(())
    }

    /// 快捷方法：记录登录
    pub async fn log_login(&self, username: &str, ip: Option<String>) {
        self.log(UserActivityLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username: username.to_string(),
            action: UserAction::Login,
            ip_address: ip,
            request_id: None,
            extra: None,
        })
        .await;
    }

    /// 快捷方法：记录聊天请求
    pub async fn log_chat_request(
        &self,
        username: &str,
        model: &str,
        message_count: usize,
        tokens_estimated: Option<u32>,
    ) {
        self.log(UserActivityLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username: username.to_string(),
            action: UserAction::ChatRequest {
                model: model.to_string(),
                message_count,
                tokens_estimated,
            },
            ip_address: None,
            request_id: None,
            extra: None,
        })
        .await;
    }

    /// 快捷方法：记录配额检查
    pub async fn log_quota_check(&self, username: &str, used: u32, remaining: u32) {
        self.log(UserActivityLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username: username.to_string(),
            action: UserAction::QuotaCheck { used, remaining },
            ip_address: None,
            request_id: None,
            extra: None,
        })
        .await;
    }

    /// 快捷方法：记录配额耗尽
    pub async fn log_quota_exceeded(&self, username: &str, used: u32, limit: u32) {
        self.log(UserActivityLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username: username.to_string(),
            action: UserAction::QuotaExceeded { used, limit },
            ip_address: None,
            request_id: None,
            extra: None,
        })
        .await;
    }

    /// 快捷方法：记录速率限制
    pub async fn log_rate_limited(&self, username: &str) {
        self.log(UserActivityLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username: username.to_string(),
            action: UserAction::RateLimited,
            ip_address: None,
            request_id: None,
            extra: None,
        })
        .await;
    }

    /// 快捷方法：记录错误
    pub async fn log_error(&self, username: &str, error_type: &str, message: &str) {
        self.log(UserActivityLog {
            timestamp: chrono::Utc::now().to_rfc3339(),
            username: username.to_string(),
            action: UserAction::Error {
                error_type: error_type.to_string(),
                message: message.to_string(),
            },
            ip_address: None,
            request_id: None,
            extra: None,
        })
        .await;
    }
}

/// 清理用户名中的非法字符，防止路径穿越
fn sanitize_username(username: &str) -> String {
    username
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 清理旧的日志文件（保留最近 10 个文件）
async fn cleanup_old_logs(user_log_dir: &PathBuf, username: &str) -> anyhow::Result<()> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(user_log_dir).await?;
    
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            // 只处理当前用户的日志文件
            if file_name.starts_with(username) && file_name.ends_with(".log") {
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    if let Ok(modified) = metadata.modified() {
                        entries.push((path.clone(), modified));
                    }
                }
            }
        }
    }

    // 按修改时间排序（最新的在前）
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    // 保留最近 10 个文件，删除其余的
    const MAX_FILES: usize = 10;
    if entries.len() > MAX_FILES {
        for (path, _) in entries.iter().skip(MAX_FILES) {
            if let Err(e) = tokio::fs::remove_file(path).await {
                tracing::warn!("删除旧日志文件失败 {:?}: {}", path, e);
            } else {
                tracing::info!("清理旧日志文件: {:?}", path);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_username() {
        assert_eq!(sanitize_username("admin"), "admin");
        assert_eq!(sanitize_username("user-123"), "user-123");
        assert_eq!(sanitize_username("user_test"), "user_test");
        assert_eq!(sanitize_username("../etc/passwd"), "___etc_passwd");
        assert_eq!(sanitize_username("user@example.com"), "user_example_com");
    }

    #[tokio::test]
    async fn test_log_creation() {
        let temp_dir = std::env::temp_dir().join("test_user_logs");
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        let logger = UserActivityLogger::new(&temp_dir);
        logger.log_login("test_user", Some("127.0.0.1".to_string())).await;

        // 检查用户目录是否创建
        let user_dir = temp_dir.join("test_user");
        assert!(user_dir.exists());
        
        // 检查日志文件是否存在
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_file = user_dir.join(format!("test_user.{}.log", today));
        assert!(log_file.exists());

        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
