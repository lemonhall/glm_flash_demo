# DeepSeek Proxy – Code Review

## Critical Findings

- **Bug – Quota bootstrap ignores runtime users** (`src/quota/manager.rs:58-64`, `src/admin/handler.rs:107-118`): `QuotaManager::load_or_init` falls back to `config.auth.users` whenever a quota snapshot is missing. Users created through the admin API live only under `data/users/*.toml`, so their first proxied request hits the unauthorized branch and never creates a quota record. Result: freshly created accounts cannot call `/chat/completions` without a manual JSON seed. Load user metadata from the persisted files (or the in-memory `UserManager`) before consulting the static config.

- **Bug – Quota charged before work starts** (`src/proxy/handler.rs:28-54`): The quota counter increments before acquiring the concurrency permit or confirming the upstream call succeeded. If the semaphore is exhausted you still spend quota and receive HTTP 429; similarly, network failures charge users for requests that never reached DeepSeek. Acquire the permit first and only persist the increment after the upstream request has begun streaming (or roll back on error).

- **Bug – Streaming timeout too aggressive** (`src/deepseek/client.rs:16-25`, `config.toml:15-23`): `reqwest::Client::builder().timeout(...)` applies to the entire request lifecycle. With the default 60 s it aborts long SSE completions mid-stream, surfacing as 502 errors. Drop the global timeout (use `timeout(None)`) or swap to `read_timeout`/`connect_timeout` so prolonged streams remain alive while still enforcing connection deadlines.

- **Security – Plaintext credential storage** (`src/auth/user_manager.rs:110-116`, `data/users/*.toml`): Passwords are stored and compared in clear text. A filesystem leak or log statement exposes reusable credentials. Hash passwords (e.g., Argon2/bcrypt) when saving and verify hashes during login; clear any existing plaintext fixtures once the migration is in place.

## High Impact Issues

- **Bug – Environment override uses the wrong key** (`src/config.rs:155-163`): The loader only respects `OPENAI_API_KEY`, yet the service (and docs) rely on `GLM_FLASH_API_KEY`/`DeepSeek` terminology. Anyone providing only the expected DeepSeek variable boots with an empty key and hits the “未设置” panic. Accept the correct env var (keep aliases if needed) before failing fast.

- **Design – `monthly_reset_day` ignored** (`src/quota/manager.rs:251-270`, `config.toml:22`): `QuotaManager::next_month_reset` always schedules resets for the first day of the next month, silently discarding the configured `monthly_reset_day`. Respect the configured reset day (and clamp invalid values) to keep billing windows aligned with ops expectations.

## Medium Impact / Performance

- **Performance – Mutex held across filesystem awaits** (`src/quota/manager.rs:36-55`): `load_or_init` keeps the `Mutex` guard while awaiting `tokio::fs::read_to_string`, serialising all quota checks behind disk I/O. Clone the state path, drop the lock, and only re-lock to insert the result.

- **Resilience – Upstream timeout classification missing** (`src/deepseek/client.rs:62-74`, `src/error.rs:47-68`): Every `reqwest::Error`, including timeouts, maps to `AppError::GlmError` (HTTP 502). Distinguish `err.is_timeout()` and return `GatewayTimeout` so clients and monitoring can react appropriately.

- **Design – Configured rate limiting unused** (`src/config.rs:9-14`, `src/config.rs:92-101`): The service exposes `rate_limit` knobs but no queueing/limiting logic ever consumes them. Either wire the queue & token bucket into `proxy` or remove the dead config to avoid false expectations.

## Suggested Next Steps

1. Fix quota initialisation and charge sequencing, then backfill any orphaned users in `data/users` with quota snapshots.
2. Rework the HTTP client timeout strategy and add regression tests that stream beyond the previous limit.
3. Introduce password hashing plus a migration path; rotate existing credentials once deployed.
4. Align configuration handling (`GLM_FLASH_API_KEY`, `monthly_reset_day`, rate limiting) so documentation and behaviour stay in sync, and add automated coverage where feasible.



 🚨 Critical（严重）- 需要立即修复

  ✅ 已修复

  - ❌ 环境变量key错误 - 已改为 OPENAI_API_KEY
  - ✅ 配额持久化阻塞 - 已修复 save_one 的锁外IO

  ❌ 未修复（严重！）

  1. 新创建用户无法使用 🔥
    - 位置：src/quota/manager.rs:58-64
    - 问题：通过 Admin API 创建的用户，第一次请求时配额系统会从 config.auth.users 查找，找不到就报错 "用户不存在"
    - 影响：新用户完全无法使用代理服务
    - 修复：应该从 UserManager 读取用户信息
  2. 配额扣费时机错误 🔥
    - 位置：src/proxy/handler.rs:28-54
    - 问题：在获取并发permit 之前就扣费了，如果被限流（429），用户白白浪费配额
    - 影响：用户配额不准确，被限流也要扣费
    - 修复：应该在获得permit且确认上游请求成功后再扣费
  3. 流式超时太激进 🔥
    - 位置：src/deepseek/client.rs:16-25
    - 问题：60秒超时会中断长时间的SSE流，导致502错误
    - 影响：长对话会被强制中断
    - 修复：应该使用 connect_timeout 而不是全局 timeout
  4. 明文密码存储 🔒
    - 位置：data/users/*.toml
    - 问题：密码明文存储，文件泄漏就完了
    - 影响：安全风险
    - 修复：应该使用 Argon2/bcrypt 哈希

  ⚠️ High Impact（高影响）

  1. monthly_reset_day 被忽略 ❌
    - 我们还没修复这个
    - 配置的重置日期不生效

  📊 Medium Impact（中等）

  1. load_or_init 持有锁时读文件 ❌
    - src/quota/manager.rs:36-55
    - 我们修复了 save_one，但 load_or_init 还有同样的问题
  2. rate_limit 配置未使用 ❌
    - 配置存在但没有实际逻辑使用

  ---
  最严重的是前3个bug（新用户无法使用、配额扣费错误、流式超时），这些会直接影响服务可用性。你希望我优先修复哪些？