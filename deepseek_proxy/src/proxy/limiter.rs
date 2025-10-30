use crate::error::AppError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

/// Token 限流器 - 每个 token 同时只允许一个请求
#[derive(Clone)]
pub struct TokenLimiter {
    /// 每个 token 的信号量（value=1 表示只允许一个并发）
    semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
}

impl TokenLimiter {
    pub fn new() -> Self {
        Self {
            semaphores: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取指定 token 的许可
    pub async fn acquire(&self, token: &str) -> Result<TokenPermit, AppError> {
        // 获取或创建该 token 的信号量
        let semaphore = {
            let mut map = self.semaphores.lock().await;
            map.entry(token.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(1)))
                .clone()
        };

        // 尝试获取许可（立即失败，不等待）
        let permit = semaphore
            .try_acquire_owned()
            .map_err(|_| {
                tracing::warn!("Token {} 已有请求正在处理", &token[..10]);
                AppError::TooManyRequests
            })?;

        tracing::debug!("Token {} 获得处理许可", &token[..10]);

        Ok(TokenPermit { _permit: permit })
    }
}

/// Token 许可证
pub struct TokenPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// 登录限流器 - 每个用户每分钟只能登录一次，返回同一个 token
#[derive(Clone)]
pub struct LoginLimiter {
    /// 用户名 -> (token, 过期时间)
    cache: Arc<Mutex<HashMap<String, (String, Instant)>>>,
    /// token 有效期
    ttl: Duration,
}

impl LoginLimiter {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds.min(60)), // 最多 60 秒
        }
    }

    /// 获取或生成 token
    /// 如果 1 分钟内已经登录过，返回缓存的 token
    pub async fn get_or_generate<F, E>(&self, username: &str, generate_fn: F) -> Result<String, E>
    where
        F: FnOnce() -> Result<String, E>,
    {
        let now = Instant::now();
        let mut cache = self.cache.lock().await;

        // 检查缓存
        if let Some((token, expires_at)) = cache.get(username) {
            if now < *expires_at {
                tracing::debug!("用户 {} 使用缓存 token", username);
                return Ok(token.clone());
            }
        }

        // 生成新 token
        let token = generate_fn()?;
        let expires_at = now + self.ttl;
        cache.insert(username.to_string(), (token.clone(), expires_at));

        tracing::debug!("用户 {} 生成新 token，有效期 {} 秒", username, self.ttl.as_secs());

        Ok(token)
    }

    /// 清理过期缓存（可选，定期调用）
    pub async fn cleanup(&self) {
        let now = Instant::now();
        let mut cache = self.cache.lock().await;
        cache.retain(|_, (_, expires_at)| now < *expires_at);
    }
}
