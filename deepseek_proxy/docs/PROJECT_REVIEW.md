# Project Review

## High Severity
- **Environment variable mismatch blocks startup** — `src/config.rs:150-158` only honours `OPENAI_API_KEY`, yet the published docs/config instruct contributors to set `GLM_FLASH_API_KEY` (`README.md:7-19`, `config.toml:28-33`). On a fresh setup the service exits with “OPENAI_API_KEY 未设置”, even if the documented variable is present. Align the names or support both to prevent boot failures.
- **Per-user concurrency gate releases too early** — `src/proxy/handler.rs:47-74` acquires a `TokenPermit` but drops it as soon as the response is constructed. The SSE body streams after the function returns, so a second request can enter while the first is still active. This contradicts the log guarantee “每个 token 同时只允许1个请求” logged in `src/main.rs:35-45` and can overload upstream. Retain the permit for the full stream lifetime (e.g., wrap it in a pinned stream or attach via `Body::from_stream` with `try_fold`).

## Medium Severity
- **Inactive accounts still authenticate** — Login checks at `src/auth/handler.rs:21-43` ignore the `is_active` flag defined in `config.toml:9-26`. Deactivated users (or compromised credentials) continue to receive tokens. Add an explicit `user.is_active` check before issuing tokens, or drop the flag to avoid false expectations.
- **Monthly reset settings ignored** — Although `config.toml:52-55` exposes `monthly_reset_day`, `QuotaManager::next_month_reset` in `src/quota/manager.rs:252-268` always schedules the reset for the first day of the next month. Any non-default value is silently ignored, leading to incorrect billing windows. Incorporate the configured day when computing `reset_at`.
- **Quota persistence blocks the hot path** — `src/quota/manager.rs:204-222` keeps the global `Mutex<HashMap<…>>` locked while awaiting `tokio::fs::write`/`rename`. Every quota check contends on this mutex, so disk latency stalls all requests once a save triggers (currently every five calls per `config.toml:52-55`). Clone the state, drop the guard, then perform I/O to avoid head-of-line blocking.

## Low Severity / Observations
- **Misleading quota comment** — `config.toml:52-54` sets `save_interval = 5` but the inline comment claims “每100次请求写一次磁盘”. Update the comment or value to prevent operational confusion.



  高严重度问题（High Severity）

  1. 环境变量名称不匹配导致启动失败 - 代码中使用 OPENAI_API_KEY，但文档要求设置 GLM_FLASH_API_KEY，导致即使设置了文档中的变量，服务也会启动失败。
  2. 并发控制过早释放 - 在 src/proxy/handler.rs:47-74 中，TokenPermit 在构造响应后就被释放了，但 SSE 流式响应还在继续，这违反了"每个 token
  同时只允许1个请求"的保证，可能导致上游服务过载。

  中等严重度问题（Medium Severity）

  1. 不活跃账户仍可认证 - 登录检查忽略了 is_active 标志，已停用的用户仍能获取令牌。
  2. 月度重置日期设置被忽略 - monthly_reset_day 配置项被忽略，系统总是在每月第一天重置，导致账单周期不正确。
  3. 配额持久化阻塞请求 - 写入配额数据时持有全局锁，磁盘 I/O 延迟会阻塞所有请求。

  低严重度问题（Low Severity）

  1. 误导性注释 - config.toml 注释声称"每100次请求写一次磁盘"，但实际配置值是 5。