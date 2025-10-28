use crate::error::AppError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 简单的限流器 - 基于信号量和延迟释放
#[derive(Clone)]
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    queue_capacity: usize,
    queue_timeout: Duration,
    delay_per_request: Duration,
    waiting_count: Arc<AtomicUsize>,  // 统计正在等待的请求数
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
            waiting_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// 尝试获取处理许可
    pub async fn acquire(&self) -> Result<RateLimitPermit, AppError> {
        // 1. 增加等待计数
        let waiting = self.waiting_count.fetch_add(1, Ordering::SeqCst);
        
        // 2. 检查队列是否已满（超过队列容量）
        if waiting >= self.queue_capacity {
            self.waiting_count.fetch_sub(1, Ordering::SeqCst);
            tracing::warn!("队列已满: 当前等待 {} 个请求, 容量 {}", waiting, self.queue_capacity);
            return Err(AppError::TooManyRequests);
        }

        // 3. 尝试获取许可，带超时
        let result = tokio::time::timeout(
            self.queue_timeout,
            self.semaphore.clone().acquire_owned(),
        )
        .await;
        
        // 4. 减少等待计数
        self.waiting_count.fetch_sub(1, Ordering::SeqCst);
        
        // 5. 处理结果
        let permit = result
            .map_err(|_| {
                tracing::warn!("请求排队超时: 等待超过 {} 秒", self.queue_timeout.as_secs());
                AppError::QueueTimeout
            })?
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
