# GLM 代理服务 - 快速启动指南

## 📦 已生成文件清单

```
glm_proxy/
├── Cargo.toml              # Rust 依赖配置
├── config.toml             # 服务配置文件
├── .env.example            # 环境变量示例
├── .gitignore              # Git 忽略文件
├── README.md               # 完整使用文档
├── test.ps1                # PowerShell 测试脚本
└── src/
    ├── main.rs             # 主程序入口
    ├── config.rs           # 配置加载
    ├── error.rs            # 错误处理
    ├── auth/
    │   ├── mod.rs          # 认证模块
    │   ├── handler.rs      # 登录接口
    │   ├── jwt.rs          # JWT 生成/验证
    │   └── middleware.rs   # Token 验证中间件
    ├── proxy/
    │   ├── mod.rs          # 代理模块
    │   ├── handler.rs      # 代理处理器
    │   └── limiter.rs      # 限流器
    └── glm/
        ├── mod.rs          # GLM 模块
        └── client.rs       # GLM API 客户端
```

## 🚀 立即开始

### 前置条件

确保已安装 Rust 工具链:
```bash
# 检查是否已安装
rustc --version

# 如果未安装，访问: https://rustup.rs/
```

### 第一步：配置 API Key

```powershell
# 在 PowerShell 中设置环境变量
$env:GLM_FLASH_API_KEY = "your-api-key-here"

# 或编辑 config.toml 文件中的 glm.api_key
```

### 第二步：编译并运行

```powershell
cd glm_proxy

# 首次编译会下载依赖，需要等待几分钟
cargo run
```

看到以下输出表示成功：
```
🚀 GLM 代理服务启动成功: http://0.0.0.0:8080
📝 登录接口: POST http://0.0.0.0:8080/auth/login
🔄 代理接口: POST http://0.0.0.0:8080/chat/completions
```

### 第三步：测试

新开一个终端窗口，运行测试脚本：
```powershell
.\test.ps1
```

## 🎯 核心功能

### ✅ 已实现特性

1. **Token 认证**
   - JWT 令牌，60秒有效期
   - 用户名密码登录
   - Bearer Token 验证

2. **请求队列与限流**
   - 队列容量: 20
   - 限流: 2 req/s
   - 排队超时: 5s
   - GLM 总超时: 20s

3. **流式代理**
   - SSE 格式透传
   - 实时响应
   - 超时自动截断

4. **错误处理**
   - 401: Token 无效
   - 408: 排队超时
   - 429: 队列满
   - 504: GLM 超时

## 📊 设计指标

| 指标 | 值 |
|------|-----|
| 并发处理能力 | 2 req/s |
| 队列容量 | 20 个请求 |
| 排队超时 | 5 秒 |
| GLM 响应超时 | 20 秒 |
| Token 有效期 | 60 秒 |
| 并发成功率提升 | 20% → 80%+ |

## 🔧 下一步

### 1. 修改默认用户

编辑 `config.toml`:
```toml
[[auth.users]]
username = "your_username"
password = "your_password"
```

### 2. 调整限流参数

```toml
[rate_limit]
requests_per_second = 2  # 根据 GLM API 限制调整
queue_capacity = 20      # 队列大小
queue_timeout_seconds = 5 # 排队超时
```

### 3. 生产部署

```bash
# 编译优化版本
cargo build --release

# 二进制文件位于
./target/release/glm_proxy
```

## 💡 使用技巧

### Python 客户端集成

只需修改 `glm_client.py` 的 BASE_URL:

```python
class GLMClient:
    BASE_URL = "http://localhost:8080"  # 指向代理服务
    
    def __init__(self, token: str, timeout: float = 60.0):
        # token 从登录接口获取
        self.api_key = token
        # ... 其他不变
```

### 并发测试

```powershell
# 模拟 10 个并发请求
1..10 | ForEach-Object -Parallel {
    .\test.ps1
} -ThrottleLimit 10
```

## 📖 更多文档

- 完整 API 文档: 查看 `README.md`
- 设计文档: 查看 `../RUST_PROXY_DESIGN.md`
- 测试示例: 运行 `.\test.ps1`

---

**享受更安全、更高效的 GLM API 调用体验！** 🎉
