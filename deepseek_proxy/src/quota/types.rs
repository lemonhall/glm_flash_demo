use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 配额档次
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaTier {
    Basic,    // 500次/月
    Pro,      // 1000次/月
    Premium,  // 1500次/月
}

impl QuotaTier {
    /// 获取配额上限
    pub fn limit(&self) -> u32 {
        match self {
            QuotaTier::Basic => 500,
            QuotaTier::Pro => 1000,
            QuotaTier::Premium => 1500,
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
        reset_at: DateTime<Utc>,
    },
    /// 配额已耗尽，需要付费
    Exceeded {
        used: u32,
        limit: u32,
        reset_at: DateTime<Utc>,
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
