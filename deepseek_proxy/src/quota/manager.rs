use super::types::{QuotaState, QuotaStateAtomic, QuotaStatus, QuotaTier};
use crate::config::Config;
use crate::error::AppError;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// 配额管理器（优化版：使用 DashMap + 原子操作）
pub struct QuotaManager {
    /// 内存缓存: username -> QuotaStateAtomic
    /// 使用 DashMap 实现无锁并发访问，不同用户的操作互不阻塞
    cache: Arc<DashMap<String, Arc<QuotaStateAtomic>>>,

    /// 配置
    config: Arc<Config>,

    /// 用户管理器（用于获取动态用户信息）
    user_manager: Arc<crate::auth::UserManager>,

    /// 数据目录
    data_dir: PathBuf,

    /// 写入间隔（每N次请求写一次）
    save_interval: u32,
}

impl QuotaManager {
    pub fn new(
        config: Arc<Config>,
        user_manager: Arc<crate::auth::UserManager>,
        data_dir: PathBuf,
        save_interval: u32,
    ) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            config,
            user_manager,
            data_dir,
            save_interval,
        }
    }

    /// 懒加载用户配额（优化版：使用 DashMap 的 entry API）
    async fn load_or_init(&self, username: &str) -> Result<Arc<QuotaStateAtomic>, AppError> {
        // 1. 快速检查内存缓存
        if let Some(state) = self.cache.get(username) {
            return Ok(state.clone());
        }

        // 2. 尝试从磁盘加载（无锁 IO）
        let file_path = self.data_dir.join(format!("{}.json", username));
        let state = if file_path.exists() {
            let content = tokio::fs::read_to_string(&file_path)
                .await
                .map_err(|e| AppError::InternalError(format!("读取配额文件失败: {}", e)))?;

            let state: QuotaState = serde_json::from_str(&content)
                .map_err(|e| AppError::InternalError(format!("解析配额数据失败: {}", e)))?;

            QuotaStateAtomic::from_state(state)
        } else {
            // 3. 首次访问，从 UserManager 获取用户信息
            let user = self.user_manager
                .get_user(username)
                .await
                .ok_or_else(|| AppError::Unauthorized(format!("用户 {} 不存在", username)))?;

            let tier = QuotaTier::from_str(&user.quota_tier)
                .ok_or_else(|| AppError::InternalError("无效的配额档次".to_string()))?;

            tracing::info!("初始化用户 {} 的配额：档次={}, 限额={}", username, user.quota_tier, tier.limit(&self.config.quota.tiers));

            let reset_at = Self::next_month_reset()
                .map_err(|e| AppError::InternalError(format!("重置时间计算失败: {}", e)))?;

            QuotaStateAtomic::from_state(QuotaState {
                username: username.to_string(),
                tier: tier.as_str().to_string(),
                monthly_limit: tier.limit(&self.config.quota.tiers),
                used_count: 0,
                last_saved_count: 0,
                reset_at,
                last_saved_at: None,
                dirty: true,
            })
        };

        // 4. 使用 DashMap 的 entry API 保证原子插入（避免竞态条件）
        let state_arc = Arc::new(state);
        self.cache
            .entry(username.to_string())
            .or_insert_with(|| state_arc.clone());

        Ok(state_arc)
    }

    /// 只检查配额（不扣费）- 优化版：无锁读取
    pub async fn check_quota(&self, username: &str) -> Result<QuotaStatus, AppError> {
        // 确保用户数据已加载
        let state = self.load_or_init(username).await?;

        let reset_at_str = state.reset_at.read().await.clone();
        let reset_at = DateTime::parse_from_rfc3339(&reset_at_str)
            .map_err(|e| AppError::InternalError(format!("解析重置时间失败: {}", e)))?;

        let used = state.get_used();
        let limit = state.monthly_limit;

        // 只检查，不递增
        if used >= limit {
            Ok(QuotaStatus::Exceeded {
                used,
                limit,
                reset_at,
            })
        } else {
            Ok(QuotaStatus::Ok {
                used,
                limit,
                remaining: limit - used,
                reset_at,
            })
        }
    }

    /// 递增配额（在确认请求成功后调用）- 优化版：原子操作
    pub async fn increment_quota(&self, username: &str) -> Result<(), AppError> {
        // 确保用户数据已加载
        let state = self.load_or_init(username).await?;

        let now = Utc::now();

        // 检查是否需要月度重置
        let need_reset = {
            let reset_at_str = state.reset_at.read().await.clone();
            let reset_at = DateTime::parse_from_rfc3339(&reset_at_str)
                .map_err(|e| AppError::InternalError(format!("解析重置时间失败: {}", e)))?;
            now > reset_at.with_timezone(&Utc)
        };

        if need_reset {
            tracing::info!("用户 {} 配额月度重置", username);

            let new_reset_at = Self::next_month_reset()
                .map_err(|e| AppError::InternalError(format!("重置时间计算失败: {}", e)))?;

            state.reset(new_reset_at).await;

            // 重置时立即保存
            self.save_one_immediately(username, &state).await?;
        }

        // 原子递增计数（无锁操作）
        let current_used = state.increment();
        let last_saved = state.get_last_saved();

        // 每 N 次保存一次
        if current_used - last_saved >= self.save_interval {
            tracing::debug!(
                "用户 {} 达到保存间隔 ({}/{}), 写入磁盘",
                username,
                current_used - last_saved,
                self.save_interval
            );

            state.update_last_saved(current_used);
            *state.last_saved_at.write().await = Some(crate::utils::now_beijing_rfc3339());

            self.save_one(username, &state).await?;
        }

        Ok(())
    }

    /// 查询配额信息（不递增）- 优化版
    pub async fn get_quota(&self, username: &str) -> Result<QuotaState, AppError> {
        let state = self.load_or_init(username).await?;
        Ok(state.to_state().await)
    }

    /// 保存单个用户数据 - 优化版：直接接受 Arc<QuotaStateAtomic>
    async fn save_one(&self, username: &str, state: &Arc<QuotaStateAtomic>) -> Result<(), AppError> {
        // 转换为可序列化的 QuotaState
        let quota_state = state.to_state().await;

        // 磁盘 I/O（不阻塞其他用户的配额操作）
        let file_path = self.data_dir.join(format!("{}.json", username));
        let temp_path = file_path.with_extension("tmp");

        // 原子写入：先写临时文件，再重命名
        let json = serde_json::to_string_pretty(&quota_state)
            .map_err(|e| AppError::InternalError(format!("序列化配额数据失败: {}", e)))?;

        tokio::fs::write(&temp_path, json)
            .await
            .map_err(|e| AppError::InternalError(format!("写入配额文件失败: {}", e)))?;

        tokio::fs::rename(temp_path, file_path)
            .await
            .map_err(|e| AppError::InternalError(format!("重命名配额文件失败: {}", e)))?;

        Ok(())
    }

    /// 立即保存（重置、关闭时使用）
    async fn save_one_immediately(&self, username: &str, state: &Arc<QuotaStateAtomic>) -> Result<(), AppError> {
        self.save_one(username, state).await
    }

    /// 保存所有数据（优雅关闭时调用）- 优化版：使用 DashMap snapshot
    pub async fn save_all(&self) -> Result<(), AppError> {
        // DashMap 支持无锁迭代，获取所有用户的快照
        let users_snapshot: Vec<(String, Arc<QuotaStateAtomic>)> = self.cache
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        for (username, state) in users_snapshot {
            tracing::info!("保存用户 {} 的配额数据", username);
            self.save_one(&username, &state).await?;
        }

        Ok(())
    }

    /// 计算下个月1号 0点（东八区 UTC+8）
    fn next_month_reset() -> Result<String, String> {
        let now = crate::utils::now_beijing();
        let next_month = if now.month() == 12 {
            NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
                .ok_or_else(|| "下年度1月1日创建失败".to_string())?
        } else {
            NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
                .ok_or_else(|| "下月1日创建失败".to_string())?
        };

        let naive_datetime = next_month.and_hms_opt(0, 0, 0)
            .ok_or_else(|| "时间00:00:00创建失败".to_string())?;

        // 创建东八区时间
        let beijing_offset = chrono::FixedOffset::east_opt(8 * 3600)
            .ok_or_else(|| "时区创建失败".to_string())?;
        let datetime: DateTime<chrono::FixedOffset> = DateTime::from_naive_utc_and_offset(naive_datetime, beijing_offset);

        Ok(datetime.to_rfc3339())
    }
}
