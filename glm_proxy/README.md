# GLM 代理服务

基于 Rust + Axum 的 GLM API 代理服务，提供 Token 认证、请求队列和限流功能。

## 快速开始

### 1. 配置 API Key

编辑 `config.toml` 或设置环境变量：

```powershell
# Windows PowerShell
$env:GLM_FLASH_API_KEY = "your-api-key-here"
```

或创建 `.env` 文件：
```bash
GLM_FLASH_API_KEY=your-api-key-here
```

### 2. 编译运行

```bash
# 开发模式
cargo run

# 生产模式 (优化编译)
cargo build --release
./target/release/glm_proxy
```

服务默认启动在 `http://0.0.0.0:8080`

## API 使用

### 1. 登录获取 Token

```bash
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "user1",
    "password": "pass123"
  }'
```

响应：
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJh...",
  "expires_in": 60
}
```

### 2. 使用 Token 调用 Chat 接口

```bash
curl -X POST http://localhost:8080/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "glm-4.5-flash",
    "messages": [
      {"role": "user", "content": "你好"}
    ],
    "temperature": 0.95,
    "stream": true
  }'
```

## Python 客户端使用

修改现有的 Python 客户端，指向代理服务：

```python
from glm_client import GLMClient

# 1. 先登录获取 token
import requests
login_resp = requests.post(
    "http://localhost:8080/auth/login",
    json={"username": "user1", "password": "pass123"}
).json()

token = login_resp["token"]

# 2. 使用 token 调用代理
class ProxyGLMClient(GLMClient):
    BASE_URL = "http://localhost:8080"  # 指向代理
    
    def __init__(self, token: str, timeout: float = 60.0):
        self.api_key = token  # 使用 token 而非 API Key
        self.timeout = timeout
        # ... 其他代码不变

# 使用
with ProxyGLMClient(token=token) as client:
    for text in client.chat(
        messages=[{"role": "user", "content": "你好"}],
        model="glm-4.5-flash"
    ):
        print(text, end="", flush=True)
```

## 配置说明

编辑 `config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8080

[auth]
# 用户列表 (生产环境建议使用数据库)
[[auth.users]]
username = "user1"
password = "pass123"

jwt_secret = "your-secret-key-change-in-production"
token_ttl_seconds = 60

[glm]
api_key = ""  # 从环境变量读取
base_url = "https://open.bigmodel.cn/api/paas/v4"
timeout_seconds = 20

[rate_limit]
requests_per_second = 2
queue_capacity = 20
queue_timeout_seconds = 5
```

## 错误码说明

| 状态码 | 说明 | 建议 |
|--------|------|------|
| 401 | Token 无效/过期 | 重新登录获取新 token |
| 408 | 排队超时 | 等待 2-3 秒后重试 |
| 429 | 队列已满 | 等待 3-5 秒后重试 |
| 504 | GLM API 超时 | 等待 5-10 秒后重试 |

## 性能特性

- **并发限制**: 2 req/s (符合 GLM API 限制)
- **队列容量**: 20 个请求
- **排队超时**: 5 秒
- **GLM 总超时**: 20 秒 (包含流式输出全过程)
- **Token 有效期**: 60 秒

## 日志

查看详细日志：
```bash
RUST_LOG=debug cargo run
```

## 生产部署

```bash
# 1. 编译优化版本
cargo build --release

# 2. 复制到部署目录
cp target/release/glm_proxy /usr/local/bin/
cp config.toml /etc/glm_proxy/

# 3. 使用 systemd 守护进程
sudo systemctl enable glm-proxy
sudo systemctl start glm-proxy
```
