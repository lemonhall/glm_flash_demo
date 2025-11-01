use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// 全局速率限制器 - 使用令牌桶算法
/// 适用于小型服务器（1核1G），防止 DoS 攻击
#[derive(Clone)]
pub struct GlobalRateLimiter {
    state: Arc<Mutex<TokenBucket>>,
    config: RateLimitConfig,
}

#[derive(Clone)]
pub struct RateLimitConfig {
    /// 每秒可处理的请求数
    pub requests_per_second: usize,
    /// 最大突发容量（令牌桶大小）
    pub burst_capacity: usize,
}

struct TokenBucket {
    /// 当前可用令牌数
    tokens: f64,
    /// 上次补充令牌的时间
    last_refill: Instant,
}

impl GlobalRateLimiter {
    /// 创建新的全局速率限制器
    pub fn new(requests_per_second: usize) -> Self {
        // 突发容量设为 RPS 的 2 倍，允许短时间突发
        let burst_capacity = requests_per_second * 2;
        
        Self {
            state: Arc::new(Mutex::new(TokenBucket {
                tokens: burst_capacity as f64,
                last_refill: Instant::now(),
            })),
            config: RateLimitConfig {
                requests_per_second,
                burst_capacity,
            },
        }
    }

    /// 尝试获取一个令牌
    /// 返回 Ok(()) 如果成功，返回 Err 包含重试等待时间（秒）
    pub async fn acquire(&self) -> Result<(), f64> {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        
        // 计算需要补充的令牌数
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        let tokens_to_add = elapsed * self.config.requests_per_second as f64;
        
        // 补充令牌，但不超过桶容量
        state.tokens = (state.tokens + tokens_to_add).min(self.config.burst_capacity as f64);
        state.last_refill = now;
        
        // 尝试消耗一个令牌
        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            tracing::debug!(
                "全局速率限制：通过（剩余令牌 {:.2}/{}）",
                state.tokens,
                self.config.burst_capacity
            );
            Ok(())
        } else {
            // 计算需要等待多久才能获得下一个令牌
            let wait_time = (1.0 - state.tokens) / self.config.requests_per_second as f64;
            tracing::warn!(
                "全局速率限制：请求过多（剩余令牌 {:.2}），建议等待 {:.2}秒",
                state.tokens,
                wait_time
            );
            Err(wait_time)
        }
    }

    /// 获取当前配置信息（用于日志）
    pub fn info(&self) -> String {
        format!(
            "全局限流: {}/秒, 突发容量: {}",
            self.config.requests_per_second, self.config.burst_capacity
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = GlobalRateLimiter::new(10); // 10 req/s
        
        // 前 20 个请求应该通过（突发容量 = 20）
        for i in 0..20 {
            assert!(
                limiter.acquire().await.is_ok(),
                "第 {} 个请求应该通过",
                i + 1
            );
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_limit() {
        let limiter = GlobalRateLimiter::new(5); // 5 req/s, burst=10
        
        // 消耗所有突发容量
        for _ in 0..10 {
            limiter.acquire().await.ok();
        }
        
        // 下一个请求应该被限制
        assert!(limiter.acquire().await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_refills_over_time() {
        let limiter = GlobalRateLimiter::new(10); // 10 req/s
        
        // 消耗所有令牌
        for _ in 0..20 {
            limiter.acquire().await.ok();
        }
        
        // 应该被限制
        assert!(limiter.acquire().await.is_err());
        
        // 等待 0.2 秒，应该补充 ~2 个令牌
        sleep(Duration::from_millis(200)).await;
        
        assert!(limiter.acquire().await.is_ok());
        assert!(limiter.acquire().await.is_ok());
    }
}
