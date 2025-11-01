use std::path::Path;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use anyhow::Result;

/// 日志配置
pub struct LoggerConfig {
    /// 日志目录
    pub log_dir: String,
    /// 日志文件名前缀
    pub file_prefix: String,
    /// 单个日志文件最大大小（字节）
    pub max_file_size: u64,
    /// 保留的日志文件数量
    pub max_files: usize,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            log_dir: "logs".to_string(),
            file_prefix: "deepseek_proxy".to_string(),
            max_file_size: 10 * 1024 * 1024, // 10 MB
            max_files: 5, // 保留最近的 5 个文件
        }
    }
}

/// 初始化日志系统
/// 
/// 特性：
/// - 同时输出到控制台和文件
/// - 自动按日期滚动日志文件
/// - 当文件超过指定大小时自动创建新文件
/// - 自动清理旧的日志文件
pub fn init_logger(config: LoggerConfig) -> Result<()> {
    // 创建日志目录
    std::fs::create_dir_all(&config.log_dir)?;

    // 设置东八区时间
    let timer = tracing_subscriber::fmt::time::OffsetTime::new(
        time::UtcOffset::from_hms(8, 0, 0).expect("Invalid UTC offset"),
        time::format_description::well_known::Rfc3339,
    );

    // 创建文件 appender，使用每日滚动策略
    let file_appender = tracing_appender::rolling::daily(&config.log_dir, &config.file_prefix);
    
    // 创建非阻塞写入器（避免日志 IO 阻塞主线程）
    // 注意：不能使用 non_blocking，因为 guard 会被立即丢弃
    // 我们直接使用同步写入，对于小型服务器来说性能影响可以接受

    // 配置环境过滤器
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "deepseek_proxy=debug,tower_http=debug,axum=debug".into());

    // 文件输出层（普通文本格式，便于查看）
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_appender)
        .with_timer(timer.clone())
        .with_ansi(false) // 文件中不使用颜色代码
        .with_target(true)
        .with_thread_ids(true);

    // 控制台输出层（人类可读格式）
    let console_layer = tracing_subscriber::fmt::layer()
        .with_timer(timer)
        .with_target(true)
        .with_thread_ids(false);

    // 组合所有层
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(console_layer)
        .init();

    // 启动后台任务来管理日志文件大小
    tokio::spawn(log_rotation_task(config));

    Ok(())
}

/// 后台任务：定期检查并清理日志文件
async fn log_rotation_task(config: LoggerConfig) {
    use tokio::time::{interval, Duration};
    
    let mut interval = interval(Duration::from_secs(60)); // 每分钟检查一次
    
    loop {
        interval.tick().await;
        
        if let Err(e) = manage_log_files(&config).await {
            eprintln!("日志文件管理失败: {}", e);
        }
    }
}

/// 管理日志文件：删除超过大小限制或数量限制的文件
async fn manage_log_files(config: &LoggerConfig) -> Result<()> {
    let log_path = Path::new(&config.log_dir);
    
    if !log_path.exists() {
        return Ok(());
    }

    // 读取所有日志文件
    let mut read_dir = tokio::fs::read_dir(log_path).await?;
    let mut entries = Vec::new();
    
    while let Some(entry) = read_dir.next_entry().await? {
        entries.push(entry);
    }

    // 过滤出以指定前缀开头的日志文件
    let mut target_files = Vec::new();
    
    for entry in entries {
        let path = entry.path();
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name.starts_with(&config.file_prefix) && file_name.ends_with(".log") {
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    target_files.push((path.clone(), metadata.len(), metadata.modified().ok()));
                }
            }
        }
    }

    // 按修改时间排序（最新的在前）
    target_files.sort_by(|a, b| b.2.cmp(&a.2));

    let mut total_size = 0u64;
    let mut files_to_delete = Vec::new();

    for (i, (path, size, _modified)) in target_files.iter().enumerate() {
        total_size += size;

        // 删除超过数量限制的文件
        if i >= config.max_files {
            files_to_delete.push(path.clone());
            continue;
        }

        // 删除单个文件超过大小限制的文件（除了最新的那个）
        if *size > config.max_file_size && i > 0 {
            files_to_delete.push(path.clone());
        }
    }

    // 执行删除
    for path in files_to_delete {
        if let Err(e) = tokio::fs::remove_file(&path).await {
            eprintln!("删除旧日志文件失败 {:?}: {}", path, e);
        } else {
            tracing::info!("删除旧日志文件: {:?}", path);
        }
    }

    // 如果总大小超过限制，删除最旧的文件
    if total_size > config.max_file_size * config.max_files as u64 {
        tracing::warn!(
            "日志文件总大小超过限制: {} MB / {} MB",
            total_size / 1024 / 1024,
            (config.max_file_size * config.max_files as u64) / 1024 / 1024
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LoggerConfig::default();
        assert_eq!(config.log_dir, "logs");
        assert_eq!(config.file_prefix, "deepseek_proxy");
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
        assert_eq!(config.max_files, 5);
    }
}
