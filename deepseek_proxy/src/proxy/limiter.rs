use crate::error::AppError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use std::collections::VecDeque;

/// 真正的队列+限流：每秒从队列取N个请求
#[derive(Clone)]
pub struct RateLimiter {
    queue: Arc<Mutex<VecDeque<tokio::sync::oneshot::Sender<()>>>>,
    queue_capacity: usize,
    requests_per_second: usize,
    queue_timeout: Duration,
}

impl RateLimiter {
    pub fn new(requests_per_second: usize, queue_capacity: usize, queue_timeout_seconds: u64) -> Self {
        let limiter = Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            queue_capacity,
            requests_per_second,
            queue_timeout: Duration::from_secs(queue_timeout_seconds),
        };
        
        // 启动后台任务：每秒从队列取N个请求
        let queue_clone = limiter.queue.clone();
        let rate = requests_per_second;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                
                let mut queue = queue_clone.lock().await;
                let count = rate.min(queue.len());
                
                if count > 0 {
                    tracing::debug!("每秒定时器触发，从队列取出 {} 个请求（队列剩余 {}）", count, queue.len() - count);
                }
                
                for _ in 0..count {
                    if let Some(tx) = queue.pop_front() {
                        let _ = tx.send(()); // 发送许可
                    }
                }
            }
        });
        
        limiter
    }

    /// 获取处理许可
    /// 
    /// 流程：
    /// 1. 检查队列是否已满，满了返回429
    /// 2. 加入队列尾部
    /// 3. 等待后台任务发送许可（每秒取N个）
    /// 4. 超时5秒返回408
    pub async fn acquire(&self) -> Result<RateLimitPermit, AppError> {
        // 1. 检查队列容量
        {
            let queue = self.queue.lock().await;
            if queue.len() >= self.queue_capacity {
                tracing::warn!("队列已满: 当前 {} 个请求, 容量 {}", queue.len(), self.queue_capacity);
                return Err(AppError::TooManyRequests);
            }
        }
        
        // 2. 创建oneshot channel
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        // 3. 加入队列尾部
        {
            let mut queue = self.queue.lock().await;
            queue.push_back(tx);
            tracing::debug!("请求加入队列，当前队列长度: {}", queue.len());
        }
        
        // 4. 等待许可（超时5秒）
        tokio::time::timeout(self.queue_timeout, rx)
            .await
            .map_err(|_| {
                tracing::warn!("请求排队超时: 等待超过 {} 秒", self.queue_timeout.as_secs());
                AppError::QueueTimeout
            })?
            .map_err(|_| AppError::Internal("接收许可失败".to_string()))?;
        
        Ok(RateLimitPermit {})
    }
}

/// 限流许可证
pub struct RateLimitPermit {}
