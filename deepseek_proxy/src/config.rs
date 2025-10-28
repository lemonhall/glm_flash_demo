use serde::Deserialize;
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub deepseek: DeepSeekConfig,
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub quota: QuotaConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub users: Vec<User>,
    pub jwt_secret: String,
    pub token_ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub username: String,
    pub password: String,
    #[serde(default = "default_quota_tier")]
    pub quota_tier: String,  // "basic", "pro", "premium"
    #[serde(default = "default_is_active")]
    pub is_active: bool,
}

fn default_quota_tier() -> String {
    "basic".to_string()
}

fn default_is_active() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_second: usize,
    pub queue_capacity: usize,
    pub queue_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuotaConfig {
    #[serde(default = "default_save_interval")]
    pub save_interval: u32,  // 每N次请求写一次磁盘
    #[serde(default = "default_monthly_reset_day")]
    pub monthly_reset_day: u32,  // 每月几号重置
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            save_interval: 100,
            monthly_reset_day: 1,
        }
    }
}

fn default_save_interval() -> u32 {
    100
}

fn default_monthly_reset_day() -> u32 {
    1
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // 加载 .env 文件 (如果存在)
        let _ = dotenvy::dotenv();

        // 加载 config.toml
        let mut config: Config = config::Config::builder()
            .add_source(config::File::with_name("config"))
            .build()?
            .try_deserialize()?;

        // 从环境变量读取 OpenAI API Key (优先级高于配置文件)
        if let Ok(api_key) = env::var("OPENAI_API_KEY") {
            config.deepseek.api_key = api_key;
        }

        // 验证必需配置
        if config.deepseek.api_key.is_empty() {
            anyhow::bail!("OPENAI_API_KEY 未设置! 请在环境变量或 .env 文件中配置");
        }

        Ok(config)
    }
}
