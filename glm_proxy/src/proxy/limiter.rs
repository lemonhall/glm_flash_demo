use crate::error::AppError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// 简单的限流器 - 基于信号量和延迟释放
#[derive(Clone)]
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    queue_capacity: usize,
    queue_timeout: Duration,
    delay_per_request: Duration,
}

impl RateLimiter {
    pub fn new(requests_per_second: usize, queue_capacity: usize, queue_timeout_seconds: u64) -> Self {
        // 初始化信号量为 requests_per_second 个许可
        let semaphore = Arc::new(Semaphore::new(requests_per_second));
        
        Self {
            semaphore,
            queue_capacity,
            queue_timeout: Duration::from_secs(queue_timeout_seconds),
            delay_per_request: Duration::from_millis(1000 / requests_per_second as u64),
        }
    }

    /// 尝试获取处理许可
    pub async fn acquire(&self) -> Result<RateLimitPermit, AppError> {
        // 检查队列是否已满
        if self.semaphore.available_permits() == 0 
            && self.queue_capacity <= (self.semaphore.available_permits()) {
            return Err(AppError::TooManyRequests);
        }

        // 尝试获取许可，带超时
        let permit = tokio::time::timeout(
            self.queue_timeout,
            self.semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| AppError::QueueTimeout)?
        .map_err(|_| AppError::Internal("获取信号量失败".to_string()))?;

        Ok(RateLimitPermit {
            _permit: permit,
            delay: self.delay_per_request,
        })
    }
}

/// 限流许可证 - Drop 时延迟释放，实现速率限制
pub struct RateLimitPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    delay: Duration,
}

impl Drop for RateLimitPermit {
    fn drop(&mut self) {
        let delay = self.delay;
        // 延迟释放许可，确保速率限制
        tokio::spawn(async move {
            sleep(delay).await;
            // permit 在这里自动释放
        });
    }
}
