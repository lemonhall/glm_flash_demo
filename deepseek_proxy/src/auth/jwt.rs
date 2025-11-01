use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // username
    pub exp: usize,       // 过期时间 (Unix timestamp)
}

pub struct JwtService {
    secret: String,
    ttl_seconds: i64,
}

impl JwtService {
    pub fn new(secret: String, ttl_seconds: u64) -> Result<Self, String> {
        let ttl_i64 = i64::try_from(ttl_seconds)
            .map_err(|_| "TTL时间溢出：超过i64最大值".to_string())?;
        
        if ttl_i64 <= 0 {
            return Err("TTL时间必须大于0".to_string());
        }
        
        Ok(Self {
            secret,
            ttl_seconds: ttl_i64,
        })
    }

    /// 生成 JWT token
    pub fn generate_token(&self, username: &str) -> anyhow::Result<String> {
        let expiration = Utc::now()
            .checked_add_signed(Duration::seconds(self.ttl_seconds))
            .ok_or_else(|| anyhow::anyhow!("时间计算溢出"))?
            .timestamp();
        
        let exp_usize = usize::try_from(expiration)
            .map_err(|_| anyhow::anyhow!("过期时间转换失败"))?;

        let claims = Claims {
            sub: username.to_string(),
            exp: exp_usize,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )?;

        Ok(token)
    }

    /// 验证 JWT token
    pub fn validate_token(&self, token: &str) -> anyhow::Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )?;

        Ok(token_data.claims)
    }

    /// 获取 token 有效期（秒）
    pub fn get_ttl_seconds(&self) -> u64 {
        self.ttl_seconds as u64
    }
}
