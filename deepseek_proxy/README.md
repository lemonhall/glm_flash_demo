# DeepSeek 代理服务

基于 Rust + Axum 的高性能 DeepSeek API 代理服务，提供用户认证、配额管理、并发控制和流式响应。

## ✨ 核心特性

- 🔐 **JWT 认证** - Token 缓存机制，60秒内复用同一Token
- 📊 **配额管理** - 按用户分配月度配额（Basic/Pro/Premium三档）
- 🚦 **并发控制** - 每个用户同时只允许1个请求，防止滥用
- 💾 **独立文件存储** - 用户配置和配额数据独立存储，支持动态修改
- 🔧 **管理接口** - 提供用户管理API（仅localhost访问）
- ⏰ **东八区时间** - 所有时间显示为北京时间（UTC+8）
- 🎯 **高性能** - 锁外IO操作，支持高并发场景

## 📁 项目结构

```
deepseek_proxy/
├── config.toml          # 主配置文件
├── data/
│   ├── users/           # 用户配置（独立文件）
│   │   ├── admin.toml
│   │   ├── user1.toml
│   │   └── user2.toml
│   └── quotas/          # 配额数据（自动生成）
│       ├── admin.json
│       ├── user1.json
│       └── user2.json
└── src/                 # 源代码
```

## 🚀 快速开始

### 1. 配置 API Key

设置环境变量（推荐）：

```powershell
# Windows PowerShell
$env:OPENAI_API_KEY = "sk-xxx"
```

```bash
# Linux/Mac
export OPENAI_API_KEY="sk-xxx"
```

或创建 `.env` 文件：
```bash
OPENAI_API_KEY=sk-xxx
```

### 2. 编译运行

```bash
# 开发模式
cargo run

# 生产模式（优化编译）
cargo build --release
./target/release/deepseek_proxy
```

服务启动在 `http://0.0.0.0:8877`

### 3. 运行测试

```bash
python test_proxy.py
```

测试包含：登录认证、流式对话、并发限流、配额管理、用户激活状态等。

## 📖 API 使用

### 用户接口

#### 1. 登录获取 Token

```bash
curl -X POST http://localhost:8877/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "password": "admin123"
  }'
```

**响应：**
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJh...",
  "expires_in": 60
}
```

**说明：**
- Token 有效期 60 秒
- 60 秒内多次登录返回同一 Token（缓存机制）
- 账户必须处于激活状态（`is_active = true`）

#### 2. 调用 Chat 接口

```bash
curl -X POST http://localhost:8877/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-chat",
    "messages": [
      {"role": "user", "content": "你好"}
    ],
    "stream": true
  }'
```

**响应：** 流式 SSE 格式

**并发限制：**
- 每个用户同时只允许 **1个请求**
- 第二个并发请求会收到 `429 Too Many Requests`
- 超时时间：60秒

**配额检查：**
- 每次请求消耗 1 次配额
- 配额耗尽返回 `402 Payment Required`
- 每月1号 00:00:00（北京时间）自动重置

### 管理接口（仅 localhost）

所有管理接口只能从 `localhost` 访问，其他来源返回 `403 Forbidden`。

#### 1. 列出所有用户

```bash
curl http://localhost:8877/admin/users
```

**响应：**
```json
{
  "users": [
    {
      "username": "admin",
      "quota_tier": "premium",
      "is_active": true
    },
    {
      "username": "user1",
      "quota_tier": "basic",
      "is_active": true
    }
  ]
}
```

#### 2. 获取用户详情

```bash
curl http://localhost:8877/admin/users/admin
```

**响应：**
```json
{
  "username": "admin",
  "quota_tier": "premium",
  "is_active": true
}
```

#### 3. 创建新用户

```bash
curl -X POST http://localhost:8877/admin/users \
  -H "Content-Type: application/json" \
  -d '{
    "username": "newuser",
    "password": "pass123",
    "quota_tier": "basic"
  }'
```

**说明：**
- 自动在 `data/users/` 目录创建用户配置文件
- 默认为激活状态（`is_active = true`）

#### 4. 设置用户激活状态

```bash
# 停用用户（逻辑删除）
curl -X POST http://localhost:8877/admin/users/user1/active \
  -H "Content-Type: application/json" \
  -d '{"is_active": false}'

# 重新激活用户
curl -X POST http://localhost:8877/admin/users/user1/active \
  -H "Content-Type: application/json" \
  -d '{"is_active": true}'
```

**说明：**
- 停用的用户无法登录
- **不提供物理删除**，只支持逻辑删除（设置 `is_active = false`）
- 用户数据永久保留，可随时重新激活

## ⚙️ 配置说明

### config.toml

```toml
[server]
host = "0.0.0.0"
port = 8877

[auth]
jwt_secret = "your-secret-key-change-in-production"
token_ttl_seconds = 60

# 用户配置存储在 data/users/ 目录（每个用户一个 .toml 文件）
# 支持动态修改，无需重启服务
# 如果需要添加初始用户，可以在这里定义 [[auth.users]]，服务首次启动时会自动导入

[deepseek]
api_key = ""  # 从环境变量 OPENAI_API_KEY 读取
base_url = "https://api.deepseek.com/v1"
timeout_seconds = 60

# HTTP客户端性能配置
[deepseek.http_client]
pool_max_idle_per_host = 20      # 连接池大小
pool_idle_timeout_seconds = 90   # 连接保活时间
connect_timeout_seconds = 10     # 连接超时
tcp_nodelay = true              # 禁用Nagle算法，降低延迟
http2_adaptive_window = true    # HTTP/2自适应窗口

[rate_limit]
requests_per_second = 2
queue_capacity = 20
queue_timeout_seconds = 5

[quota]
save_interval = 5              # 每5次请求写一次磁盘
monthly_reset_day = 1          # 每月1号重置

[quota.tiers]
basic = 500      # 基础版：500次/月
pro = 1000       # 专业版：1000次/月
premium = 1500   # 高级版：1500次/月
```

### 用户配置文件（data/users/admin.toml）

```toml
username = "admin"
password = "admin123"
quota_tier = "premium"
is_active = true
created_at = "2025-10-30T22:00:00+08:00"
updated_at = "2025-10-30T22:00:00+08:00"
```

**说明：**
- 每个用户一个独立的 `.toml` 文件
- 修改后立即生效，无需重启服务
- 时间格式为东八区（UTC+8）

### 配额数据文件（data/quotas/admin.json）

```json
{
  "username": "admin",
  "tier": "premium",
  "monthly_limit": 1500,
  "used_count": 42,
  "last_saved_count": 40,
  "reset_at": "2025-11-01T00:00:00+08:00",
  "last_saved_at": "2025-10-30T23:20:00+08:00"
}
```

**说明：**
- 自动生成和更新
- 每5次请求持久化一次（可配置）
- 服务关闭时自动保存所有脏数据

## 🛡️ 安全特性

### 1. 并发控制

- 每个用户（Token）同时只允许 **1个请求**
- 使用 `Semaphore` 实现许可证机制
- 请求完成前，第二个请求被拒绝（429）
- 超时自动释放（60秒）

### 2. 配额管理

- 按用户分配月度配额（可配置）
- 配额耗尽返回 `402 Payment Required`
- 每月1号 00:00:00（北京时间）自动重置
- 实时持久化，防止数据丢失

### 3. 管理接口隔离

- 管理 API 只能从 `localhost` 访问
- 其他来源返回 `403 Forbidden`
- 防止远程滥用

### 4. 数据持久化

- 用户配置：独立文件存储（`data/users/*.toml`）
- 配额数据：JSON 格式（`data/quotas/*.json`）
- 原子写入：先写临时文件，再重命名
- 锁外IO：不阻塞其他用户

## 📊 错误码说明

| 状态码 | 错误码 | 说明 | 建议 |
|--------|--------|------|------|
| 401 | `unauthorized` | Token 无效/过期或账户已停用 | 重新登录获取新 Token |
| 402 | `quota_exceeded` | 月度配额已耗尽 | 等待下月重置或升级套餐 |
| 404 | `not_found` | 用户不存在 | 检查用户名 |
| 408 | `queue_timeout` | 排队超时 | 等待 2-3 秒后重试 |
| 429 | `queue_full` / `too_many_requests` | 队列已满或并发超限 | 等待 3-5 秒后重试 |
| 504 | `glm_timeout` | DeepSeek API 超时 | 等待 5-10 秒后重试 |

## 🎯 性能指标

- **并发限制**: 每用户 1 req/s（全局 2 req/s）
- **Token 缓存**: 60秒复用
- **配额检查**: < 10μs（内存操作）
- **配额保存**: 异步 IO，不阻塞
- **连接池**: 20个连接/主机
- **请求超时**: 60秒

## 🔧 开发

### 日志

```bash
# 调试日志
RUST_LOG=debug cargo run

# 生产日志
RUST_LOG=info cargo run
```

### 测试

```bash
# 单元测试
cargo test

# 集成测试
python test_proxy.py
```

### 代码检查

```bash
cargo check
cargo clippy
cargo fmt
```

## 🚢 生产部署

### 编译

```bash
cargo build --release
```

### 部署

```bash
# 1. 复制到部署目录
cp target/release/deepseek_proxy /usr/local/bin/
cp config.toml /etc/deepseek_proxy/
cp -r data /etc/deepseek_proxy/

# 2. 创建 systemd 服务
cat > /etc/systemd/system/deepseek-proxy.service <<EOF
[Unit]
Description=DeepSeek Proxy Service
After=network.target

[Service]
Type=simple
User=deepseek
WorkingDirectory=/etc/deepseek_proxy
Environment="OPENAI_API_KEY=sk-xxx"
ExecStart=/usr/local/bin/deepseek_proxy
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

# 3. 启动服务
sudo systemctl daemon-reload
sudo systemctl enable deepseek-proxy
sudo systemctl start deepseek-proxy

# 4. 查看日志
sudo journalctl -u deepseek-proxy -f
```

## 📝 常见问题

### 1. 配额不准确？

检查 `config.toml` 中的 `save_interval`，建议设置为 5-10。每 N 次请求写一次磁盘。

### 2. Token 过期太快？

修改 `config.toml` 中的 `token_ttl_seconds`，默认 60 秒。

### 3. 并发请求被拒绝？

这是正常的！每个用户同时只允许 1 个请求。等待第一个请求完成或超时。

### 4. 如何删除用户？

**不支持物理删除**。使用管理接口设置 `is_active = false` 进行逻辑删除：
```bash
curl -X POST http://localhost:8877/admin/users/username/active \
  -H "Content-Type: application/json" \
  -d '{"is_active": false}'
```

### 5. 时间显示不对？

所有时间统一为东八区（UTC+8），格式为 `2025-10-30T23:20:00+08:00`。

## 📄 许可证

MIT License

## 🙏 致谢

- [Axum](https://github.com/tokio-rs/axum) - Web 框架
- [Tokio](https://tokio.rs/) - 异步运行时
- [Chrono](https://github.com/chronotope/chrono) - 时间处理
- [DeepSeek](https://www.deepseek.com/) - AI API
