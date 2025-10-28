# DeepSeek API 代理服务

基于 Rust + Axum 的 DeepSeek API 安全代理网关

## 项目概述

本项目提供了一个安全的 DeepSeek API 代理服务，解决以下问题：

1. **API 密钥隐藏**：客户端使用临时 Token 访问，真实 API Key 只保存在服务器
2. **请求串行控制**：同一 Token 同时只允许一个请求，避免并发冲突
3. **登录缓存**：60 秒内多次登录返回同一 Token，减少重复认证

## 快速开始

### 1. 配置环境变量

设置 DeepSeek API Key：

```powershell
# Windows PowerShell
[System.Environment]::SetEnvironmentVariable('OPENAI_API_KEY', 'your-api-key', 'User')

# 重启终端使环境变量生效
```

### 2. 启动代理服务

```powershell
cd deepseek_proxy
.\start.ps1
```

服务将启动在 `http://0.0.0.0:8080`

### 3. 测试服务

```powershell
cd deepseek_proxy
uv run python test_proxy.py
```

## 使用方法

### 登录获取 Token

```python
import httpx

# 登录
response = httpx.post(
    "http://localhost:8080/auth/login",
    json={"username": "admin", "password": "admin123"}
)
token = response.json()["token"]
```

### 流式对话

```python
import httpx
import json

headers = {"Authorization": f"Bearer {token}"}
data = {
    "model": "deepseek-chat",
    "messages": [
        {"role": "user", "content": "你好"}
    ],
    "stream": True
}

with httpx.stream(
    "POST",
    "http://localhost:8080/chat/completions",
    headers=headers,
    json=data,
    timeout=60
) as response:
    for line in response.iter_lines():
        if line.startswith("data: "):
            data_str = line[6:]
            if data_str.strip() == "[DONE]":
                break
            chunk = json.loads(data_str)
            if "choices" in chunk:
                content = chunk["choices"][0]["delta"].get("content", "")
                print(content, end="", flush=True)
```

## 限流规则

### 1. 登录限流

- 每个用户 **60 秒内**多次登录返回**同一个 Token**
- Token 缓存在内存中，60 秒后自动失效

### 2. 请求串行

- 同一 Token **同时只允许 1 个请求**正在处理
- 并发请求会收到 `429 Too Many Requests` 错误

### 3. 不同用户可并发

- 不同 Token 之间**不互相影响**
- 多用户可以同时请求

## 状态码说明

| 状态码 | 说明 | 处理建议 |
|--------|------|----------|
| 200 | 成功 | 正常处理 |
| 401 | Token 无效/过期 | 重新登录 |
| 429 | 该 Token 已有请求在处理 | 等待当前请求完成 |
| 502 | DeepSeek API 错误 | 稍后重试 |

## 配置文件

`deepseek_proxy/config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8080

[auth]
jwt_secret = "your-secret-key-change-in-production"
token_ttl_seconds = 3600  # Token 有效期（实际缓存60秒）

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

## 项目结构

```
deepseek_proxy/
├── src/
│   ├── main.rs                 # 服务入口
│   ├── config.rs               # 配置管理
│   ├── error.rs                # 错误定义
│   ├── auth/
│   │   ├── handler.rs          # 登录接口
│   │   ├── jwt.rs              # JWT 服务
│   │   └── middleware.rs       # Token 验证中间件
│   ├── deepseek/
│   │   └── client.rs           # DeepSeek API 客户端
│   └── proxy/
│       ├── handler.rs          # 代理接口
│       └── limiter.rs          # 限流器（登录缓存 + Token 串行）
├── config.toml                 # 配置文件
├── start.ps1                   # 启动脚本
├── test_proxy.py               # 测试脚本
└── test_deepseek.py            # DeepSeek 专用测试
```

## 测试场景

运行 `test_proxy.py` 会执行以下测试：

1. ✅ **登录认证** - 验证用户名密码登录
2. ✅ **登录缓存 (60秒)** - 验证多次登录返回同一 Token
3. ✅ **流式对话** - 验证 SSE 流式响应
4. ✅ **Token 串行限流** - 验证同一 Token 并发请求被限流
5. ✅ **多用户并发** - 验证不同 Token 可以并发
6. ✅ **基础并发测试** - 综合并发场景
7. ✅ **未授权拦截** - 验证无 Token 访问被拒绝

## 技术栈

- **Rust** - 系统编程语言
- **Axum** - 高性能 Web 框架
- **Tokio** - 异步运行时
- **Reqwest** - HTTP 客户端（支持流式响应）
- **JWT** - Token 生成与验证

## 开发调试

```powershell
# 编译
cd deepseek_proxy
cargo build

# 运行（开发模式）
cargo run

# 查看日志（debug 级别）
$env:RUST_LOG="debug"; cargo run
```

## 部署建议

```bash
# 编译 release 版本
cargo build --release

# 运行
./target/release/deepseek_proxy
```

## 常见问题

### Q: 为什么需要代理服务？

A: 
1. **安全性**：隐藏真实 API Key，避免泄露
2. **控制性**：统一管理用户权限和访问控制
3. **稳定性**：请求串行化，避免并发冲突

### Q: Token 有效期是多久？

A: Token 60 秒内有效。60 秒内多次登录会返回同一个 Token。

### Q: 429 错误如何处理？

A: 说明该 Token 已有请求正在处理。等待当前请求完成后再发起新请求。

### Q: 支持多少并发？

A: 每个 Token 同时只能处理 1 个请求。但不同用户（不同 Token）可以并发。
