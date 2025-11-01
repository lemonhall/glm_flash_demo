use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 配额档次
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaTier {
    Basic,    // 500次/月
    Pro,      // 1000次/月
    Premium,  // 1500次/月
}

impl QuotaTier {
    /// 获取配额上限（从配置中读取）
    pub fn limit(&self, config: &crate::config::QuotaTiersConfig) -> u32 {
        match self {
            QuotaTier::Basic => config.basic,
            QuotaTier::Pro => config.pro,
            QuotaTier::Premium => config.premium,
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "basic" => Some(QuotaTier::Basic),
            "pro" => Some(QuotaTier::Pro),
            "premium" => Some(QuotaTier::Premium),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            QuotaTier::Basic => "basic",
            QuotaTier::Pro => "pro",
            QuotaTier::Premium => "premium",
        }
    }
}

/// 配额检查结果
#[derive(Debug)]
pub enum QuotaStatus {
    /// 配额充足，可以继续请求
    Ok {
        used: u32,
        limit: u32,
        remaining: u32,
        reset_at: DateTime<FixedOffset>,  // 支持任意时区（东八区）
    },
    /// 配额已耗尽，需要付费
    Exceeded {
        used: u32,
        limit: u32,
        reset_at: DateTime<FixedOffset>,  // 支持任意时区（东八区）
    },
}

/// 配额状态（用于持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaState {
    pub username: String,
    pub tier: String,
    pub monthly_limit: u32,
    pub used_count: u32,
    pub last_saved_count: u32,
    pub reset_at: String,  // ISO 8601 格式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_saved_at: Option<String>,
    
    #[serde(skip)]
    pub dirty: bool,  // 是否有未保存的修改
}

/// 配额状态（原子版本，用于高并发场景）
pub struct QuotaStateAtomic {
    pub username: String,
    pub tier: String,
    pub monthly_limit: u32,
    /// 原子计数器，支持无锁并发递增
    pub used_count: Arc<AtomicU32>,
    /// 上次保存时的计数
    pub last_saved_count: Arc<AtomicU32>,
    /// 重置时间（使用 RwLock 保护，因为重置频率很低）
    pub reset_at: Arc<RwLock<String>>,
    /// 上次保存时间
    pub last_saved_at: Arc<RwLock<Option<String>>>,
}

impl QuotaStateAtomic {
    /// 从普通 QuotaState 创建
    pub fn from_state(state: QuotaState) -> Self {
        Self {
            username: state.username,
            tier: state.tier,
            monthly_limit: state.monthly_limit,
            used_count: Arc::new(AtomicU32::new(state.used_count)),
            last_saved_count: Arc::new(AtomicU32::new(state.last_saved_count)),
            reset_at: Arc::new(RwLock::new(state.reset_at)),
            last_saved_at: Arc::new(RwLock::new(state.last_saved_at)),
        }
    }

    /// 转换为普通 QuotaState（用于序列化）
    pub async fn to_state(&self) -> QuotaState {
        QuotaState {
            username: self.username.clone(),
            tier: self.tier.clone(),
            monthly_limit: self.monthly_limit,
            used_count: self.used_count.load(Ordering::Relaxed),
            last_saved_count: self.last_saved_count.load(Ordering::Relaxed),
            reset_at: self.reset_at.read().await.clone(),
            last_saved_at: self.last_saved_at.read().await.clone(),
            dirty: false,
        }
    }

    /// 原子递增使用计数
    pub fn increment(&self) -> u32 {
        self.used_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 获取当前使用计数
    pub fn get_used(&self) -> u32 {
        self.used_count.load(Ordering::Relaxed)
    }

    /// 获取上次保存的计数
    pub fn get_last_saved(&self) -> u32 {
        self.last_saved_count.load(Ordering::Relaxed)
    }

    /// 更新上次保存的计数
    pub fn update_last_saved(&self, count: u32) {
        self.last_saved_count.store(count, Ordering::Relaxed);
    }

    /// 重置配额（月度重置）
    pub async fn reset(&self, new_reset_at: String) {
        self.used_count.store(0, Ordering::Relaxed);
        self.last_saved_count.store(0, Ordering::Relaxed);
        *self.reset_at.write().await = new_reset_at;
    }
}
