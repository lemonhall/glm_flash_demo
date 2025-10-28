use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // username
    pub exp: usize,       // 过期时间 (Unix timestamp)
}

pub struct JwtService {
    secret: String,
    ttl_seconds: i64,
}

impl JwtService {
    pub fn new(secret: String, ttl_seconds: u64) -> Self {
        Self {
            secret,
            ttl_seconds: ttl_seconds as i64,
        }
    }

    /// 生成 JWT token
    pub fn generate_token(&self, username: &str) -> anyhow::Result<String> {
        let expiration = Utc::now()
            .checked_add_signed(Duration::seconds(self.ttl_seconds))
            .expect("valid timestamp")
            .timestamp() as usize;

        let claims = Claims {
            sub: username.to_string(),
            exp: expiration,
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
}
