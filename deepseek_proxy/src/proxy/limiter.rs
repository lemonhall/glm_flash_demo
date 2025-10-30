use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;


/// Token 许可证
pub struct TokenPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// 持有许可证的流包装器
/// 确保许可证在整个流的生命周期内都被持有
pub struct PermitGuardedStream<S> {
    stream: S,
    _permit: TokenPermit,
}

impl<S> PermitGuardedStream<S> {
    pub fn new(stream: S, permit: TokenPermit) -> Self {
        Self {
            stream,
            _permit: permit,
        }
    }
}

impl<S> Stream for PermitGuardedStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

/// 统一Token管理器 - 管理Token生命周期和并发控制
#[derive(Clone)]
pub struct LoginLimiter {
    /// 用户名 -> (token, semaphore, 过期时间)
    cache: Arc<Mutex<HashMap<String, (String, Arc<Semaphore>, Instant)>>>,
    /// token 有效期
    ttl: Duration,
}

impl LoginLimiter {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds), // 使用配置的值
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

        // 懒清理：清理所有过期的缓存条目
        let before_count = cache.len();
        cache.retain(|_, (_, _, expires_at)| now < *expires_at);
        let after_count = cache.len();
        let cleaned = before_count - after_count;
        
        if cleaned > 0 {
            tracing::debug!("LoginLimiter 清理了 {} 个过期缓存条目，剩余 {} 个", cleaned, after_count);
        }

        // 检查缓存
        if let Some((token, _, expires_at)) = cache.get(username) {
            if now < *expires_at {
                tracing::debug!("用户 {} 使用缓存 token", username);
                return Ok(token.clone());
            }
        }

        // 生成新 token
        let token = generate_fn()?;
        let expires_at = now + self.ttl;
        let semaphore = Arc::new(Semaphore::new(1)); // 新 token 创建新的信号量
        cache.insert(username.to_string(), (token.clone(), semaphore, expires_at));

        tracing::debug!("用户 {} 生成新 token，有效期 {} 秒", username, self.ttl.as_secs());

        Ok(token)
    }

    /// 统一获取Token和并发许可 - 一站式解决方案
    /// 既管理Token生命周期，又控制并发访问
    pub async fn get_token_and_permit<F, E>(&self, username: &str, generate_fn: F) -> Result<(String, TokenPermit), E>
    where
        F: FnOnce() -> Result<String, E>,
        E: From<crate::error::AppError>,
    {
        let now = Instant::now();
        let mut cache = self.cache.lock().await;

        // 懒清理：清理所有过期的缓存条目
        let before_count = cache.len();
        cache.retain(|_, (_, _, expires_at)| now < *expires_at);
        let after_count = cache.len();
        let cleaned = before_count - after_count;
        
        if cleaned > 0 {
            tracing::debug!("TokenManager 清理了 {} 个过期Token，剩余 {} 个", cleaned, after_count);
        }

        // 检查缓存
        if let Some((token, semaphore, expires_at)) = cache.get(username) {
            if now < *expires_at {
                // 尝试获取信号量许可
                let permit = semaphore.clone()
                    .try_acquire_owned()
                    .map_err(|_| {
                        tracing::warn!("用户 {} 的Token已有请求正在处理", username);
                        crate::error::AppError::TooManyRequests
                    })?;

                tracing::debug!("用户 {} 使用缓存Token并获得处理许可", username);
                return Ok((token.clone(), TokenPermit { _permit: permit }));
            }
        }

        // 生成新 token 和信号量
        let token = generate_fn()?;
        let expires_at = now + self.ttl;
        let semaphore = Arc::new(Semaphore::new(1));
        
        // 立即获取新Token的许可
        let permit = semaphore.clone()
            .try_acquire_owned()
            .map_err(|_| crate::error::AppError::InternalError("新Token信号量获取失败".to_string()))?;

        cache.insert(username.to_string(), (token.clone(), semaphore, expires_at));

        tracing::debug!("用户 {} 生成新Token并获得处理许可，有效期 {} 秒", username, self.ttl.as_secs());

        Ok((token, TokenPermit { _permit: permit }))
    }

    /// 通过用户名获取Token许可（用于已验证的请求）
    /// 如果用户有有效Token，返回许可；否则返回错误要求重新登录
    pub async fn acquire_permit_by_username(&self, username: &str) -> Result<TokenPermit, crate::error::AppError> {
        let now = Instant::now();
        let mut cache = self.cache.lock().await;

        // 懒清理
        cache.retain(|_, (_, _, expires_at)| now < *expires_at);

        // 查找用户的有效Token
        if let Some((_, semaphore, expires_at)) = cache.get(username) {
            if now < *expires_at {
                // 尝试获取许可
                let permit = semaphore.clone()
                    .try_acquire_owned()
                    .map_err(|_| {
                        tracing::warn!("用户 {} 已有请求正在处理", username);
                        crate::error::AppError::TooManyRequests
                    })?;

                tracing::debug!("用户 {} 获得请求处理许可", username);
                return Ok(TokenPermit { _permit: permit });
            }
        }

        // 没有有效Token，需要重新登录
        Err(crate::error::AppError::Unauthorized("Token已过期，请重新登录".to_string()))
    }

}
