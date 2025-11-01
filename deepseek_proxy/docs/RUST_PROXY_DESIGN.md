# DeepSeek API 代理服务设计文档

## 一、设计价值与意义

### 1.1 为什么需要代理层？

本代理服务解决了直接调用 DeepSeek API 的核心痛点：

#### 痛点 1: API 密钥安全暴露风险

**无代理的问题**:
```
App 客户端 ──(携带真实 API Key)──> DeepSeek API
```
- ❌ App 端代码中包含真实 API Key，容易泄露
- ❌ 密钥一旦泄露，需要更换并重新分发给所有客户端
- ❌ 无法追踪和审计 API 使用情况
- ❌ 无法对不同用户进行精细化权限控制

**使用代理后**:
```
App 客户端 ──(临时 Token)──> Rust 代理 ──(真实 API Key)──> DeepSeek API
```
- ✅ **隐藏真实密钥**: App 端永远接触不到 DeepSeek 的真实 API Key
- ✅ **可撤销访问**: Token 自动过期，无需更换密钥
- ✅ **统一认证**: 通过登录接口集中管理用户权限
- ✅ **审计日志**: 代理层可记录所有请求，追踪使用情况
- ✅ **成本控制**: 可为不同用户设置配额限制

#### 痛点 2: 并发请求冲突

**无代理时的问题**:
```
同一客户端的 2 个并发请求
├─> 可能导致流式响应混乱
└─> 难以追踪哪个响应属于哪个请求
```

**使用代理后**:
```
同一 Token 的并发请求
├─> 第 1 个请求正常处理 ✓
└─> 第 2 个请求被拦截 (429 Too Many Requests)

用户体验: 明确的串行化保证，避免响应混乱
```

### 1.2 核心价值总结

| 维度 | 无代理 | 有代理 | 提升 |
|------|--------|--------|------|
| **安全性** | API Key 暴露风险 | Token 隔离，密钥隐藏 | ⭐⭐⭐⭐⭐ |
| **请求控制** | 无并发控制 | Token 级串行化 | ⭐⭐⭐⭐⭐ |
| **登录优化** | 每次都生成新 Token | 60秒内复用同一 Token | ⭐⭐⭐⭐ |
| **可维护性** | 客户端自行处理 | 统一错误码和策略 | ⭐⭐⭐⭐ |
| **可扩展性** | 无 | 支持缓存/限流/监控 | ⭐⭐⭐⭐⭐ |

---

## 二、限流策略设计

### 2.1 登录限流（1分钟内缓存）

**设计目标**: 减少重复 Token 生成，优化性能

```
用户第1次登录 (0秒)
    ↓
生成 Token A
    ↓
缓存 (username -> Token A, 过期时间 60秒)
    ↓
用户第2次登录 (5秒)
    ↓
命中缓存，直接返回 Token A ✓
    ↓
用户第3次登录 (65秒)
    ↓
缓存过期，生成新 Token B
```

**实现**:
```rust
struct LoginLimiter {
    cache: Arc<Mutex<HashMap<String, (String, Instant)>>>,
    ttl: Duration,  // 60 秒
}

impl LoginLimiter {
    async fn get_or_generate<F>(
        &self, 
        username: &str, 
        generate_fn: F
    ) -> String 
    where F: FnOnce() -> String 
    {
        let now = Instant::now();
        let mut cache = self.cache.lock().await;
        
        // 检查缓存
        if let Some((token, expires_at)) = cache.get(username) {
            if now < *expires_at {
                return token.clone();  // 返回缓存的 token
            }
        }
        
        // 生成新 token
        let token = generate_fn();
        let expires_at = now + self.ttl;
        cache.insert(username.to_string(), (token.clone(), expires_at));
        token
    }
}
```

### 2.2 Token 串行化（同一Token同时只允许1个请求）

**设计目标**: 避免同一客户端的并发请求冲突

```
客户端发起 2 个并发请求（使用同一 Token）
    ↓
请求1: 尝试获取 Token 的 Semaphore
    ├─> 成功获取（Semaphore 初始值 = 1）
    └─> 开始处理请求
    ↓
请求2: 尝试获取同一 Token 的 Semaphore  
    ├─> 失败（Semaphore 已被占用）
    └─> 返回 429 Too Many Requests
    ↓
请求1: 处理完成，释放 Semaphore
    ↓
请求3: 可以获取 Semaphore 并处理 ✓
```

**实现**:
```rust
struct TokenLimiter {
    semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
}

impl TokenLimiter {
    async fn acquire(&self, token: &str) -> Result<TokenPermit> {
        // 获取或创建该 token 的信号量
        let semaphore = {
            let mut map = self.semaphores.lock().await;
            map.entry(token.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(1)))
                .clone()
        };

        // 尝试获取许可（非阻塞）
        let permit = semaphore.try_acquire_owned()
            .map_err(|_| AppError::TooManyRequests)?;

        Ok(TokenPermit { _permit: permit })
    }
}
```

### 2.3 多用户并发支持

**设计目标**: 不同用户（不同 Token）可以并发请求

```
用户 A (Token1)  ──┐
用户 B (Token2)  ──┼──> 代理服务 ──> DeepSeek API
用户 C (Token3)  ──┘

每个 Token 独立的 Semaphore:
Token1 Semaphore: 1/1 (使用中)
Token2 Semaphore: 1/1 (使用中)  
Token3 Semaphore: 1/1 (使用中)

结果: 3 个用户可以同时请求 ✓
```

---

## 三、技术架构

### 3.1 技术栈选型

```
- Web 框架: Axum (高性能、类型安全)
- HTTP 客户端: Reqwest (支持流式响应)
- 异步运行时: Tokio
- 并发控制: Semaphore (信号量)
- 认证: JWT (jsonwebtoken)
- 缓存: HashMap + Mutex
```

### 3.2 核心组件架构

```
┌─────────────┐
│  App Client │
└──────┬──────┘
       │ 1. POST /auth/login
       │    {username, password}
       ├──────────────────────►┌──────────────────┐
       │                        │  Auth Module     │
       │◄──────────────────────┤  - 验证用户名密码  │
       │ 2. token (60s cache)  │  - 登录缓存      │
       │                        │  - JWT 生成      │
       │                        └──────────────────┘
       │
       │ 3. POST /chat/completions
       │    Header: Authorization: Bearer {token}
       │    Body: {model, messages, stream: true}
       │
       ├──────────────────────►┌──────────────────────────┐
       │                        │  Proxy Module            │
       │                        │  ┌────────────────────┐  │
       │                        │  │ Token Validator    │  │
       │                        │  │ (JWT验证)          │  │
       │                        │  └─────────┬──────────┘  │
       │                        │            ▼             │
       │                        │  ┌────────────────────┐  │
       │                        │  │ Token Limiter      │  │
       │                        │  │ (串行化检查)       │  │
       │                        │  │ - 同Token只允许1个 │  │
       │                        │  └─────────┬──────────┘  │
       │                        │            ▼             │
       │                        │  ┌────────────────────┐  │
       │                        │  │ DeepSeek Client    │  │
       │                        │  │ - 转发请求         │  │
       │                        │  │ - 流式响应代理     │  │
       │                        │  │ - 超时: 60s       │  │
       │                        │  └────────────────────┘  │
       │                        └──────────────────────────┘
       │
       │◄───────────────────── 4. SSE Stream Response
       │ data: {"choices":[{"delta":{"content":"..."}}]}
       │ data: [DONE]
       │
```

---

## 四、核心流程设计

### 4.1 登录流程

```
POST /auth/login
Content-Type: application/json

{
    "username": "admin",
    "password": "admin123"
}

// 响应
{
    "token": "eyJ0eXAiOiJKV1QiLCJh...",
    "expires_in": 3600  // JWT过期时间（但缓存只有60秒）
}
```

**登录缓存逻辑**:
1. 检查 `username` 是否在缓存中
2. 如果缓存未过期（60秒内），直接返回缓存的 token
3. 如果缓存过期或不存在，生成新 token 并缓存

### 4.2 代理请求流程

```
客户端请求
    ↓
验证 Token (JWT)
    ↓ (无效)
    ├──→ 401 Unauthorized
    ↓ (有效)
检查该 Token 是否已有请求在处理
    ↓ (是)
    ├──→ 429 Too Many Requests
    ↓ (否)
获取该 Token 的 Semaphore 许可
    ↓
转发请求到 DeepSeek API
    ↓
流式代理响应 (超时: 60s)
    ↓ (超时)
    ├──→ 504 Gateway Timeout
    ↓ (成功)
返回给客户端
    ↓
释放 Semaphore 许可
```

### 4.3 状态码设计

| 状态码 | 说明 | 客户端行为 |
|--------|------|-----------|
| 200 | 成功 | 正常处理 |
| 401 | Token 无效/过期 | 重新登录获取 token |
| 429 | 该 Token 已有请求在处理 | 等待当前请求完成 |
| 502 | DeepSeek API 错误 | 稍后重试 |
| 504 | DeepSeek API 超时 | 稍后重试 |

---

## 五、配置参数

```toml
[server]
host = "0.0.0.0"
port = 8080

[auth]
jwt_secret = "your-secret-key-change-in-production"
token_ttl_seconds = 3600  # JWT 有效期（缓存只有60秒）

[[auth.users]]
username = "admin"
password = "admin123"

[[auth.users]]
username = "user1"
password = "pass123"

[deepseek]
api_key = ""  # 从环境变量 OPENAI_API_KEY 读取
base_url = "https://api.deepseek.com/v1"
timeout_seconds = 60
```

---

## 六、目录结构

```
deepseek_proxy/
├── Cargo.toml
├── config.toml
├── start.ps1                # 启动脚本
└── src/
    ├── main.rs              # 入口 + 路由
    ├── config.rs            # 配置加载
    ├── error.rs             # 错误类型
    ├── auth/
    │   ├── mod.rs
    │   ├── handler.rs       # 登录接口
    │   ├── jwt.rs           # JWT 生成/验证
    │   └── middleware.rs    # Token 验证中间件
    ├── deepseek/
    │   ├── mod.rs
    │   └── client.rs        # DeepSeek API 客户端
    └── proxy/
        ├── mod.rs
        ├── handler.rs       # 代理接口
        └── limiter.rs       # 限流器（登录缓存 + Token串行）
```

---

## 七、测试用例

### 7.1 登录缓存测试

```python
# 第1次登录
token1 = login("admin", "admin123")

# 第2次登录（1秒后）
token2 = login("admin", "admin123")

# 验证
assert token1 == token2  # 60秒内返回同一 token ✓
```

### 7.2 Token 串行化测试

```python
token = login("admin", "admin123")

# 并发发送 2 个请求
def request_1():
    return chat(token, "请求1")

def request_2():
    return chat(token, "请求2")

results = ThreadPoolExecutor().map([request_1, request_2])

# 验证
assert len([r for r in results if r.status_code == 200]) == 1  # 1个成功
assert len([r for r in results if r.status_code == 429]) == 1  # 1个被限流 ✓
```

### 7.3 多用户并发测试

```python
token1 = login("admin", "admin123")
token2 = login("user1", "pass123")

# 并发发送
result1 = chat(token1, "用户1请求")
result2 = chat(token2, "用户2请求")

# 验证
assert result1.status_code == 200  # 用户1成功
assert result2.status_code == 200  # 用户2也成功 ✓
```

---

## 八、部署建议

### 8.1 编译

```powershell
# 开发模式
cargo run

# 生产模式
cargo build --release
./target/release/deepseek_proxy
```

### 8.2 环境变量

```powershell
# Windows
[System.Environment]::SetEnvironmentVariable('OPENAI_API_KEY', 'your-key', 'User')

# Linux/Mac
export OPENAI_API_KEY='your-key'
```

### 8.3 日志配置

```powershell
# Debug 日志
$env:RUST_LOG="deepseek_proxy=debug,tower_http=debug"
cargo run

# Info 日志（生产）
$env:RUST_LOG="info"
./target/release/deepseek_proxy
```

---

## 九、与现有客户端的兼容性

现有 HTTP 客户端无需修改，只需：

1. **更改 BASE_URL**:
   ```python
   BASE_URL = "http://localhost:8080"  # 使用代理
   ```

2. **添加登录步骤**:
   ```python
   # 登录获取 token
   token = login("admin", "admin123")
   
   # 使用 token 请求
   headers = {"Authorization": f"Bearer {token}"}
   ```

3. **处理 429 错误**:
   ```python
   if response.status_code == 429:
       # 等待当前请求完成后重试
       time.sleep(1)
       retry()
   ```

---

## 十、性能特性

| 特性 | 值 | 说明 |
|------|-----|------|
| **登录缓存时间** | 60 秒 | 减少重复 Token 生成 |
| **Token 并发度** | 1 | 每个 Token 同时只处理1个请求 |
| **多用户并发** | 无限制 | 不同 Token 可以并发 |
| **DeepSeek 超时** | 60 秒 | 流式响应总超时时间 |
| **内存占用** | < 10MB | Rust 高效内存管理 |

---

**设计完成，已实现并部署。**
