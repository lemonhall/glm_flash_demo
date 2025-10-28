use serde::Deserialize;
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub deepseek: DeepSeekConfig,
    pub rate_limit: RateLimitConfig,
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
