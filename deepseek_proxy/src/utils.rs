use chrono::{DateTime, FixedOffset, Utc};

/// 获取当前时间（东八区 UTC+8）
pub fn now_beijing() -> DateTime<FixedOffset> {
    let beijing = FixedOffset::east_opt(8 * 3600).expect("Invalid timezone offset");
    Utc::now().with_timezone(&beijing)
}

/// 获取当前时间的 RFC3339 字符串（东八区 UTC+8）
pub fn now_beijing_rfc3339() -> String {
    now_beijing().to_rfc3339()
}
