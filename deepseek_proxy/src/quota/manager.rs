use super::types::{QuotaState, QuotaStatus, QuotaTier};
use crate::config::Config;
use crate::error::AppError;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 配额管理器
pub struct QuotaManager {
    /// 内存缓存: username -> QuotaState
    cache: Arc<Mutex<HashMap<String, QuotaState>>>,
    
    /// 配置
    config: Arc<Config>,
    
    /// 数据目录
    data_dir: PathBuf,
    
    /// 写入间隔（每N次请求写一次）
    save_interval: u32,
}

impl QuotaManager {
    pub fn new(config: Arc<Config>, data_dir: PathBuf, save_interval: u32) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            config,
            data_dir,
            save_interval,
        }
    }

    /// 懒加载用户配额
    async fn load_or_init(&self, username: &str) -> Result<(), AppError> {
        let mut cache = self.cache.lock().await;
        
        // 1. 检查内存缓存
        if cache.contains_key(username) {
            return Ok(());
        }
        
        // 2. 尝试从磁盘加载
        let file_path = self.data_dir.join(format!("{}.json", username));
        let state = if file_path.exists() {
            let content = tokio::fs::read_to_string(&file_path)
                .await
                .map_err(|e| AppError::InternalError(format!("读取配额文件失败: {}", e)))?;
            
            let mut state: QuotaState = serde_json::from_str(&content)
                .map_err(|e| AppError::InternalError(format!("解析配额数据失败: {}", e)))?;
            
            state.dirty = false;
            state
        } else {
            // 3. 首次访问，从配置初始化
            let user = self
                .config
                .auth
                .users
                .iter()
                .find(|u| u.username == username)
                .ok_or_else(|| AppError::Unauthorized("用户不存在".to_string()))?;
            
            let tier = QuotaTier::from_str(&user.quota_tier)
                .ok_or_else(|| AppError::InternalError("无效的配额档次".to_string()))?;
            
            QuotaState {
                username: username.to_string(),
                tier: tier.as_str().to_string(),
                monthly_limit: tier.limit(&self.config.quota.tiers),
                used_count: 0,
                last_saved_count: 0,
                reset_at: Self::next_month_reset()
                    .map_err(|e| AppError::InternalError(format!("重置时间计算失败: {}", e)))?,
                last_saved_at: None,
                dirty: true,
            }
        };
        
        cache.insert(username.to_string(), state);
        Ok(())
    }

    /// 检查并递增配额（核心方法）
    pub async fn check_and_increment(&self, username: &str) -> Result<QuotaStatus, AppError> {
        // 确保用户数据已加载
        self.load_or_init(username).await?;
        
        let now = Utc::now();
        
        // 处理月度重置
        let need_reset = {
            let cache = self.cache.lock().await;
            let state = cache
                .get(username)
                .ok_or_else(|| AppError::InternalError("配额状态未找到".to_string()))?;

            let reset_at = DateTime::parse_from_rfc3339(&state.reset_at)
                .map_err(|e| AppError::InternalError(format!("解析重置时间失败: {}", e)))?;

            // 检查是否需要月度重置（比较时转换为 UTC）
            now > reset_at.with_timezone(&Utc)
        };
        
        if need_reset {
            tracing::info!("用户 {} 配额月度重置", username);
            
            // 在锁内完成重置，避免竞态条件
            let mut cache = self.cache.lock().await;
            let state = cache
                .get_mut(username)
                .ok_or_else(|| AppError::InternalError("配额状态未找到".to_string()))?;
            
            // 再次检查重置时间，防止重复重置
            let current_reset_at = DateTime::parse_from_rfc3339(&state.reset_at)
                .map_err(|e| AppError::InternalError(format!("解析重置时间失败: {}", e)))?;

            if now > current_reset_at.with_timezone(&Utc) {
                state.used_count = 0;
                state.last_saved_count = 0;
                state.reset_at = Self::next_month_reset()
                    .map_err(|e| AppError::InternalError(format!("重置时间计算失败: {}", e)))?;
                state.dirty = true;
                
                let username_clone = username.to_string();
                drop(cache);  // 在异步操作前释放锁
                
                // 重置时立即保存
                self.save_one_immediately(&username_clone).await?;
            }
        }
        
        // 检查配额并递增
        let mut cache = self.cache.lock().await;
        let state = cache
            .get_mut(username)
            .ok_or_else(|| AppError::InternalError("配额状态未找到".to_string()))?;
        
        let reset_at = DateTime::parse_from_rfc3339(&state.reset_at)
            .map_err(|e| AppError::InternalError(format!("解析重置时间失败: {}", e)))?;
        
        // 检查配额
        if state.used_count >= state.monthly_limit {
            return Ok(QuotaStatus::Exceeded {
                used: state.used_count,
                limit: state.monthly_limit,
                reset_at,
            });
        }
        
        // 递增计数
        state.used_count += 1;
        state.dirty = true;
        
        let current_used = state.used_count;
        let limit = state.monthly_limit;
        let last_saved = state.last_saved_count;
        
        // 每 N 次保存一次
        if current_used - last_saved >= self.save_interval {
            tracing::debug!(
                "用户 {} 达到保存间隔 ({}/{}), 写入磁盘",
                username,
                current_used - last_saved,
                self.save_interval
            );
            
            state.last_saved_count = current_used;
            state.last_saved_at = Some(crate::utils::now_beijing_rfc3339());
            state.dirty = false;
            
            drop(cache);  // 释放锁
            self.save_one(username).await?;
        }
        
        Ok(QuotaStatus::Ok {
            used: current_used,
            limit,
            remaining: limit - current_used,
            reset_at,
        })
    }

    /// 查询配额信息（不递增）
    pub async fn get_quota(&self, username: &str) -> Result<QuotaState, AppError> {
        self.load_or_init(username).await?;
        
        let cache = self.cache.lock().await;
        cache
            .get(username)
            .cloned()
            .ok_or_else(|| AppError::InternalError("配额状态未找到".to_string()))
    }

    /// 保存单个用户数据
    async fn save_one(&self, username: &str) -> Result<(), AppError> {
        // 1. 快速获取锁，克隆数据，立即释放锁（避免阻塞其他用户）
        let state = {
            let cache = self.cache.lock().await;
            cache
                .get(username)
                .ok_or_else(|| AppError::InternalError("配额状态未找到".to_string()))?
                .clone()  // 克隆数据
        }; // 锁在这里释放！

        // 2. 在锁外进行磁盘 I/O（不阻塞其他用户的配额检查）
        let file_path = self.data_dir.join(format!("{}.json", username));
        let temp_path = file_path.with_extension("tmp");

        // 原子写入：先写临时文件，再重命名
        let json = serde_json::to_string_pretty(&state)
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
    async fn save_one_immediately(&self, username: &str) -> Result<(), AppError> {
        self.save_one(username).await
    }

    /// 保存所有脏数据（优雅关闭时调用）
    pub async fn save_all(&self) -> Result<(), AppError> {
        let cache = self.cache.lock().await;
        let dirty_users: Vec<String> = cache
            .iter()
            .filter(|(_, state)| state.dirty)
            .map(|(username, _)| username.clone())
            .collect();
        
        drop(cache);  // 释放锁
        
        for username in dirty_users {
            tracing::info!("保存用户 {} 的配额数据", username);
            self.save_one(&username).await?;
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
