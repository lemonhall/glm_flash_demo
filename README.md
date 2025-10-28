# GLM Flash Demo

极简的智谱 AI GLM-4.5-Flash API 客户端 - 仅支持同步流式调用

## 快速开始

### 1. 安装依赖

```bash
uv sync
```

### 2. 配置 API Key

编辑 `set_api_key.ps1`，将 `YOUR_API_KEY_HERE` 替换为你的实际 API Key，然后运行:

```powershell
.\set_api_key.ps1
```

**重启终端**后环境变量即可生效。

### 3. 运行示例

```bash
uv run python main.py
```

## 使用方法

```python
from glm_client import GLMClient

# 自动从环境变量 GLM_FLASH_API_KEY 读取 API Key
with GLMClient() as client:
    for text in client.chat(
        messages=[
            {"role": "system", "content": "你是一个有用的AI助手。"},
            {"role": "user", "content": "你好"}
        ],
        model="glm-4.5-flash"
    ):
        print(text, end="", flush=True)
```

## API 参数

- `messages`: 消息列表，格式 `[{"role": "user", "content": "..."}]`
- `model`: 模型名称，默认 `"glm-4.5-flash"`
- `temperature`: 温度参数 0.0-1.0，默认 1.0
- `top_p`: 核采样参数，默认 0.95  
- `max_tokens`: 最大输出 token 数（可选）
- 其他参数: `do_sample`, `stop`, `request_id`, `user_id` 等

## 特性

✅ 极简实现 - 只保留同步流式调用  
✅ 自动从环境变量读取 API Key  
✅ 基于 httpx 的高性能 HTTP 客户端  
✅ 完整的类型标注  
✅ 上下文管理器支持
