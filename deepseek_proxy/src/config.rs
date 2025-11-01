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
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub users: Vec<User>,  // 可选，默认为空数组（用户从 data/users/ 加载）
    pub jwt_secret: String,
    pub token_ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct User {
    pub username: String,
    pub password: String,
    #[serde(default = "default_quota_tier")]
    pub quota_tier: String,  // "basic", "pro", "premium"
    #[serde(default = "default_is_active")]
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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
    #[serde(default)]
    pub http_client: HttpClientConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpClientConfig {
    #[serde(default = "default_pool_max_idle_per_host")]
    pub pool_max_idle_per_host: usize,
    #[serde(default = "default_pool_idle_timeout_seconds")]
    pub pool_idle_timeout_seconds: u64,
    #[serde(default = "default_connect_timeout_seconds")]
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_tcp_nodelay")]
    pub tcp_nodelay: bool,
    #[serde(default = "default_http2_adaptive_window")]
    pub http2_adaptive_window: bool,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            pool_max_idle_per_host: 20,
            pool_idle_timeout_seconds: 90,
            connect_timeout_seconds: 10,
            tcp_nodelay: true,
            http2_adaptive_window: true,
        }
    }
}

fn default_pool_max_idle_per_host() -> usize { 20 }
fn default_pool_idle_timeout_seconds() -> u64 { 90 }
fn default_connect_timeout_seconds() -> u64 { 10 }
fn default_tcp_nodelay() -> bool { true }
fn default_http2_adaptive_window() -> bool { true }

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_second: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_login_fail_window_seconds")]
    pub login_fail_window_seconds: u64,
    #[serde(default = "default_login_fail_threshold")]
    pub login_fail_threshold: usize,
    #[serde(default)]
    pub webhook_url: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            login_fail_window_seconds: 60,
            login_fail_threshold: 5,
            webhook_url: None,
        }
    }
}

fn default_login_fail_window_seconds() -> u64 { 60 }
fn default_login_fail_threshold() -> usize { 5 }

#[derive(Debug, Clone, Deserialize)]
pub struct QuotaConfig {
    #[serde(default = "default_save_interval")]
    pub save_interval: u32,  // 每N次请求写一次磁盘
    #[serde(default = "default_monthly_reset_day")]
    pub monthly_reset_day: u32,  // 每月几号重置
    #[serde(default)]
    pub tiers: QuotaTiersConfig,  // 配额档次限制
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuotaTiersConfig {
    #[serde(default = "default_basic_quota")]
    pub basic: u32,
    #[serde(default = "default_pro_quota")]
    pub pro: u32,
    #[serde(default = "default_premium_quota")]
    pub premium: u32,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            save_interval: 100,
            monthly_reset_day: 1,
            tiers: QuotaTiersConfig::default(),
        }
    }
}

impl Default for QuotaTiersConfig {
    fn default() -> Self {
        Self {
            basic: 500,
            pro: 1000,
            premium: 1500,
        }
    }
}

fn default_save_interval() -> u32 { 100 }
fn default_monthly_reset_day() -> u32 { 1 }
fn default_basic_quota() -> u32 { 500 }
fn default_pro_quota() -> u32 { 1000 }
fn default_premium_quota() -> u32 { 1500 }

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
