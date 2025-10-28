# GLM API 代理服务设计文档

## 一、设计价值与意义

### 1.1 为什么需要代理层？

本代理服务解决了直接调用 GLM API 的两个核心痛点：

#### 痛点 1: API 密钥安全暴露风险

**无代理的问题**:
```
App 客户端 ──(携带真实 API Key)──> GLM API
```
- ❌ App 端代码中包含真实 API Key，容易泄露
- ❌ 密钥一旦泄露，需要更换并重新分发给所有客户端
- ❌ 无法追踪和审计 API 使用情况
- ❌ 无法对不同用户进行精细化权限控制

**使用代理后**:
```
App 客户端 ──(临时 Token)──> Rust 代理 ──(真实 API Key)──> GLM API
```
- ✅ **隐藏真实密钥**: App 端永远接触不到 GLM 的真实 API Key
- ✅ **可撤销访问**: Token 60秒过期，自动失效，无需更换密钥
- ✅ **统一认证**: 通过登录接口集中管理用户权限
- ✅ **审计日志**: 代理层可记录所有请求，追踪使用情况
- ✅ **成本控制**: 可为不同用户设置配额限制

#### 痛点 2: 严格的并发限制导致低可用性

GLM API 限制: **2 requests/second**

**无代理时的问题**:
```
10 个请求同时到达 GLM
├─> 2 个成功 ✓
└─> 8 个失败 ✗ (429 Too Many Requests)

成功率: 20%
用户体验: 大量请求失败，需要客户端自行实现复杂重试逻辑
```

**使用代理后**:
```
10 个请求同时到达代理
├─> 2 个立即处理 ✓
├─> 8 个进入队列排队 (容量20，最多等待5秒)
│   └─> 按 2req/s 速度依次处理
│       ├─> 大部分在5秒内完成 ✓
│       └─> 超时的返回 408，客户端快速重试
└─> 超出队列容量的返回 429

成功率: 80%+ (取决于请求时间分布)
用户体验: 透明排队，自动削峰填谷
```

### 1.2 核心价值总结

| 维度 | 无代理 | 有代理 | 提升 |
|------|--------|--------|------|
| **安全性** | API Key 暴露风险 | Token 隔离，密钥隐藏 | ⭐⭐⭐⭐⭐ |
| **并发成功率** | 20% (瞬时10并发) | 80%+ | **4倍提升** |
| **用户体验** | 频繁429错误 | 透明排队处理 | ⭐⭐⭐⭐⭐ |
| **可维护性** | 客户端自行重试 | 统一错误码和策略 | ⭐⭐⭐⭐ |
| **可扩展性** | 无 | 支持缓存/限流/监控 | ⭐⭐⭐⭐⭐ |

### 1.3 削峰填谷效果示意

```
时间轴:
  0s    1s    2s    3s    4s    5s
  ↓     ↓     ↓     ↓     ↓     ↓

客户端请求 (突发): 
  ████████████                      (10个请求瞬时到达)

代理队列缓冲:
  ▓▓▓▓▓▓▓▓▓▓                        (排队等待)

GLM API 实际负载:
  ▓▓  ▓▓  ▓▓  ▓▓  ▓▓              (稳定 2req/s)
  0s  1s  2s  3s  4s
```

**本质**: 用**队列**把**瞬时并发**转化为**时间上的串行处理**，在 GLM 的 2req/s 限制下，最大化系统吞吐量和可用性。

---

## 二、需求概述

构建一个 Rust 中间代理服务，提供以下功能：

1. **身份验证**: 客户端先登录获取临时 token（1分钟有效期）
2. **请求代理**: 客户端使用 token 访问代理服务，代理转发到 GLM API
3. **并发控制**: GLM API 限制 2 req/s，需要队列排队
4. **超时处理**: 排队超时(5秒)或 GLM 响应超时(20秒)都需要优雅处理
5. **流式响应**: 保持与 Python 客户端相同的 SSE 流式接口

## 二、技术架构

### 2.1 技术栈选型

```
- Web 框架: axum (高性能、类型安全、易于使用)
- HTTP 客户端: reqwest (支持流式响应)
- 并发控制: tokio (异步运行时) + semaphore (信号量限流)
- Token 管理: dashmap (并发安全的 HashMap)
- 限流策略: governor (令牌桶算法)
```

### 2.2 核心组件

```
┌─────────────┐
│  App Client │
└──────┬──────┘
       │ 1. POST /auth/login
       ├──────────────────────►┌──────────────────┐
       │                        │  Auth Module     │
       │◄──────────────────────┤  - 验证用户名密码  │
       │ 2. token (1min TTL)   │  - 生成临时token  │
       │                        └──────────────────┘
       │
       │ 3. POST /chat/completions
       │    Header: Authorization: Bearer {token}
       │    Body: {messages, model, ...}
       │
       ├──────────────────────►┌──────────────────────────┐
       │                        │  Proxy Module            │
       │                        │  ┌────────────────────┐  │
       │                        │  │ Token Validator    │  │
       │                        │  └─────────┬──────────┘  │
       │                        │            ▼             │
       │                        │  ┌────────────────────┐  │
       │                        │  │ Request Queue      │  │
       │                        │  │ - 队列大小: 20     │  │
       │                        │  │ - 排队超时: 5s    │  │
       │                        │  └─────────┬──────────┘  │
       │                        │            ▼             │
       │                        │  ┌────────────────────┐  │
       │                        │  │ Rate Limiter       │  │
       │                        │  │ - 限制: 2 req/s    │  │
       │                        │  └─────────┬──────────┘  │
       │                        │            ▼             │
       │                        │  ┌────────────────────┐  │
       │                        │  │ GLM API Client     │  │
       │                        │  │ - 转发请求         │  │
       │                        │  │ - 流式响应代理     │  │
       │                        │  │ - 超时: 20s       │  │
       │                        │  └────────────────────┘  │
       │                        └──────────────────────────┘
       │
       │◄───────────────────── 4. SSE Stream Response
       │ data: {"choices":[{"delta":{"content":"..."}}]}
       │ data: [DONE]
       │
```

## 三、核心流程设计

### 3.1 认证流程

```rust
POST /auth/login
Content-Type: application/json

{
    "username": "user1",
    "password": "pass123"
}

// 响应
{
    "token": "eyJ0eXAiOiJKV1QiLCJh...",
    "expires_in": 60
}
```

**Token 结构**:
- 使用 JWT (jsonwebtoken crate)
- Payload: `{ sub: username, exp: timestamp }`
- 过期时间: 60 秒

### 3.2 代理请求流程

```
客户端请求
    ↓
验证 Token (有效期检查)
    ↓ (无效)
    ├──→ 401 Unauthorized
    ↓ (有效)
尝试加入队列 (容量: 20)
    ↓ (队列满)
    ├──→ 429 Too Many Requests (客户端等待 3-5 秒后重试)
    ↓ (成功入队)
等待信号量 (限流: 2 req/s)
    ↓ (排队超时 5s)
    ├──→ 408 Request Timeout (客户端等待 2-3 秒后重试)
    ↓ (获得许可)
转发请求到 GLM API
    ↓
流式代理响应 (超时: 20s)
    ↓ (GLM 超时)
    ├──→ 504 Gateway Timeout
    ↓ (成功)
返回给客户端
```

### 3.3 状态码设计

| 状态码 | 说明 | 客户端行为 |
|--------|------|-----------|
| 200 | 成功 | 正常处理 |
| 401 | Token 无效/过期 | 重新登录获取 token |
| 408 | 排队超时 | 等待 2-3 秒后重试 |
| 429 | 队列已满 | 等待 3-5 秒后重试 |
| 504 | GLM API 超时 | 等待 5-10 秒后重试 |

### 3.4 流式响应处理

对于 SSE 流式响应,代理服务需要:

1. **透传流式数据**: 逐块转发 GLM 返回的 SSE 数据
2. **超时监控**: 在流式传输过程中监控总耗时
3. **错误处理**: GLM 连接断开时优雅关闭客户端连接

```rust
// 伪代码示例
async fn proxy_stream(client_req) {
    let start_time = Instant::now();
    let mut glm_stream = forward_to_glm(client_req).await?;
    
    while let Some(chunk) = glm_stream.next().await {
        // 检查总超时 - 20秒总耗时限制
        if start_time.elapsed() > Duration::from_secs(20) {
            return Err(GatewayTimeout);
        }
        
        // 转发数据块
        yield chunk;
    }
}
```

## 四、配置参数

```toml
[server]
host = "0.0.0.0"
port = 8080

[auth]
# 简化版：硬编码用户（生产环境应使用数据库）
users = [
    { username = "user1", password = "pass123" },
    { username = "user2", password = "pass456" }
]
jwt_secret = "your-secret-key-change-in-production"
token_ttl_seconds = 60

[glm]
api_key = "${GLM_FLASH_API_KEY}"  # 从环境变量读取
base_url = "https://open.bigmodel.cn/api/paas/v4"
timeout_seconds = 20

[rate_limit]
requests_per_second = 2
queue_capacity = 20  # 限流 2req/s × 超时 5s × 安全系数 2 = 20
queue_timeout_seconds = 5
```

## 五、目录结构

```
glm_proxy/
├── Cargo.toml
├── .env
├── config.toml
└── src/
    ├── main.rs              # 入口 + 路由
    ├── auth/
    │   ├── mod.rs
    │   ├── handler.rs       # 登录接口
    │   ├── jwt.rs           # JWT 生成/验证
    │   └── middleware.rs    # Token 验证中间件
    ├── proxy/
    │   ├── mod.rs
    │   ├── handler.rs       # 代理接口
    │   ├── queue.rs         # 请求队列
    │   └── limiter.rs       # 限流器
    ├── glm/
    │   ├── mod.rs
    │   └── client.rs        # GLM API 客户端
    ├── config.rs            # 配置加载
    └── error.rs             # 错误类型定义
```

## 六、关键实现细节

### 6.1 并发控制策略

**队列容量计算**:
- 限流: 2 req/s
- 排队超时: 5s
- 理论最大处理: 2 × 5 = 10 个请求
- 安全系数: 2倍
- **最终容量: 20**

**简化逻辑**:
```rust
// 伪代码 - 非常简单的入队逻辑
async fn try_enqueue(request: Request) -> Result<()> {
    // 尝试获取队列位置
    if queue.len() >= 20 {
        // 队列满，直接返回 429
        return Err(TooManyRequests);
    }
    
    // 入队成功，等待限流器
    let permit = rate_limiter
        .acquire_with_timeout(Duration::from_secs(5))
        .await?;  // 超时返回 408
    
    // 获得许可，转发请求
    Ok(())
}
```

使用 **信号量 (Semaphore)** 实现精确限流:

```rust
use tokio::sync::Semaphore;
use std::sync::Arc;

struct RateLimiter {
    semaphore: Arc<Semaphore>,
    permits_per_second: u32,
}

impl RateLimiter {
    fn new(rate: u32) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(rate as usize)),
            permits_per_second: rate,
        }
    }
    
    async fn acquire_with_timeout(&self, timeout: Duration) -> Result<()> {
        tokio::time::timeout(
            timeout,
            self.semaphore.acquire()
        ).await??;
        
        // 延迟释放许可,实现速率限制
        let sem = self.semaphore.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            drop(permit); // 1秒后释放
        });
        
        Ok(())
    }
}
```

### 6.2 流式响应代理

```rust
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;

async fn proxy_chat_stream(
    req: ChatRequest,
    glm_client: &GlmClient,
) -> Result<impl Stream<Item = Result<Event, Error>>> {
    let glm_stream = glm_client.chat_stream(req).await?;
    
    let stream = glm_stream.map(|chunk| {
        match chunk {
            Ok(data) => Ok(Event::default().data(data)),
            Err(e) => Err(e.into()),
        }
    });
    
    Ok(stream)
}
```

### 6.3 超时分层设计

1. **排队超时**: 5 秒(快速失败,避免客户端长时间等待)
2. **GLM 总响应超时**: 20 秒(包含流式输出全过程,超时主动截断)
3. **Token 有效期**: 60 秒(安全性考虑)

## 七、测试要点

1. **并发压力测试**: 模拟 10+ 并发请求,验证限流效果
2. **超时场景**: 模拟 GLM API 慢响应,验证超时处理
3. **Token 过期**: 验证过期 token 被拒绝
4. **流式完整性**: 验证 SSE 数据完整传输

## 八、部署建议

```bash
# 编译优化版本
cargo build --release

# 使用 systemd 守护进程
sudo systemctl enable glm-proxy
sudo systemctl start glm-proxy

# 反向代理 (Nginx)
location /api/ {
    proxy_pass http://127.0.0.1:8080/;
    proxy_buffering off;  # 重要: 禁用缓冲以支持 SSE
}
```

## 九、与 Python 客户端的兼容性

Python 客户端无需修改,只需更改 BASE_URL:

```python
class GLMClient:
    # BASE_URL = "https://open.bigmodel.cn/api/paas/v4"  # 原始
    BASE_URL = "http://localhost:8080"  # 使用代理
    
    def __init__(self, api_key: Optional[str] = None, timeout: float = 60.0):
        # api_key 改为 token (从 /auth/login 获取)
        if api_key is None:
            api_key = os.getenv("GLM_PROXY_TOKEN")
        # ... 其他代码不变
```

---

**设计完成,准备开始编码实现。**
