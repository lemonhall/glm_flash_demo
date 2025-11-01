use dashmap::DashMap;
use std::time::{Duration, Instant};
use crate::config::SecurityConfig;

// 记录失败尝试 (username:ip -> Vec<Instant>) 简易实现
pub struct BruteForceGuard {
    attempts: DashMap<String, Vec<Instant>>,
    cfg: SecurityConfig,
}

impl BruteForceGuard {
    pub fn new(cfg: SecurityConfig) -> Self { Self { attempts: DashMap::new(), cfg } }

    fn key(username: &str, ip: &str) -> String { format!("{}:{}", username, ip) }

    pub fn record_failure(&self, username: &str, ip: &str) -> usize {
        let now = Instant::now();
        let window = Duration::from_secs(self.cfg.login_fail_window_seconds);
        let key = Self::key(username, ip);
        let mut vec = self.attempts.entry(key).or_insert_with(Vec::new);
        // 清理过期
        vec.retain(|t| now.duration_since(*t) <= window);
        vec.push(now);
        vec.len()
    }

    pub fn should_block(&self, username: &str, ip: &str) -> bool {
        let key = Self::key(username, ip);
        if let Some(vec) = self.attempts.get(&key) {
            vec.len() >= self.cfg.login_fail_threshold
        } else { false }
    }

    pub fn reset_on_success(&self, username: &str, ip: &str) {
        let key = Self::key(username, ip);
        self.attempts.remove(&key);
    }
}
