use crate::{error::AppError, AppState};
use axum::{extract::{State, ConnectInfo}, Json};
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: u64,
}

pub async fn login(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // 0. 全局速率限制检查（防止登录接口被暴力破解）
    if let Err(wait_time) = state.global_rate_limiter.acquire().await {
        tracing::warn!("全局速率限制：拒绝登录请求，建议等待 {:.2} 秒", wait_time);
        return Err(AppError::TooManyRequests);
    }

    // 验证用户名密码（从内存中的用户管理器获取）
    let client_ip = addr.ip().to_string();

    // 暴力破解阻断检查（在真正验证前先看是否已被阻断）
    if state.brute_force_guard.should_block(&req.username, &client_ip) {
        crate::metrics::METRICS.login_bruteforce_blocked.inc();
        tracing::warn!(user=%req.username, ip=%client_ip, "登录被暴力破解策略阻断");
        // 可选 webhook 通知
        if let Some(url) = &state.config.security.webhook_url {
            spawn_webhook_notify(url.clone(), "login_bruteforce_blocked", &req.username, &client_ip, None);
        }
        return Err(AppError::TooManyRequests);
    }

    let user = match state
        .user_manager
        .find_user(&req.username, &req.password)
        .await
    {
        Some(u) => u,
        None => {
            let fails = state.brute_force_guard.record_failure(&req.username, &client_ip);
            crate::metrics::METRICS.login_attempts.with_label_values(&["failure"]).inc();
            tracing::warn!(user=%req.username, ip=%client_ip, fails=fails, "登录失败");
            if state.brute_force_guard.should_block(&req.username, &client_ip) {
                crate::metrics::METRICS.login_bruteforce_blocked.inc();
                if let Some(url) = &state.config.security.webhook_url {
                    spawn_webhook_notify(url.clone(), "login_bruteforce_blocked", &req.username, &client_ip, Some(fails));
                }
                return Err(AppError::TooManyRequests);
            }
            return Err(AppError::Unauthorized("用户名或密码错误".to_string()));
        }
    };

    // 检查账户是否已激活
    if !user.is_active {
        tracing::warn!("用户 {} 尝试登录，但账户已被停用", user.username);
        return Err(AppError::Unauthorized("账户已被停用".to_string()));
    }

    // 使用登录限流器：在有效期内返回同一个 token（最多 60 秒）
    let token = state.login_limiter
        .get_or_generate(&user.username, || {
            state
                .jwt_service
                .generate_token(&user.username)
                .map_err(|e| AppError::InternalError(format!("Token生成失败: {}", e)))
        })
        .await?;

    // 记录登录行为
    state.activity_logger.log_login(&user.username, None).await;
    tracing::info!("用户 {} 登录成功", user.username);
    crate::metrics::METRICS.login_attempts.with_label_values(&["success"]).inc();
    state.brute_force_guard.reset_on_success(&user.username, &client_ip);

    Ok(Json(LoginResponse {
        token,
        expires_in: state.jwt_service.get_ttl_seconds(),  // 返回实际的 TTL（已被限制为最多 60 秒）
    }))
}

fn spawn_webhook_notify(url: String, event: &str, username: &str, ip: &str, fail_count: Option<usize>) {
    let event = event.to_string();
    let username = username.to_string();
    let ip = ip.to_string();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "event": event,
            "username": username,
            "ip": ip,
            "fail_count": fail_count,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        if let Err(e) = reqwest::Client::new().post(&url).json(&payload).send().await {
            tracing::warn!(error=%e, "Webhook 通知发送失败");
        }
    });
}
