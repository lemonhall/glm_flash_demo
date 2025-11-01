# 用户行为日志说明

## 功能概述

每个用户的操作行为都会被详细记录在独立的日志文件中，便于审计和问题追踪。

## 日志结构

```
logs/
└── users/
    ├── admin/
    │   ├── admin.2025-11-01.log
    │   ├── admin.2025-11-01.153045.log  (滚动后的归档)
    │   └── admin.2025-11-02.log
    ├── user1/
    │   ├── user1.2025-11-01.log
    │   └── user1.2025-11-02.log
    └── test_user/
        └── test_user.2025-11-01.log
```

### 目录结构说明
- **每个用户独立文件夹**: `logs/users/{username}/`
- **按日期自动滚动**: 每天自动创建新文件 `{username}.{date}.log`
- **按大小自动滚动**: 单个文件超过 5MB 时自动归档
- **自动清理**: 每个用户保留最近 10 个日志文件

## 日志滚动机制

### 1. 按日期滚动
每天自动创建新的日志文件：
- `admin.2025-11-01.log` (今天)
- `admin.2025-11-02.log` (明天)

### 2. 按大小滚动
当日志文件超过 5MB 时：
- 原文件重命名为: `admin.2025-11-01.153045.log` (添加时间戳)
- 创建新文件: `admin.2025-11-01.log`

### 3. 自动清理
- 每个用户保留最近 10 个日志文件
- 自动删除更早的日志文件

## 记录的行为类型

### 1. 登录 (Login)
```json
{
  "timestamp": "2025-11-01T12:34:56.789012+00:00",
  "username": "admin",
  "action": "login",
  "ip_address": "192.168.1.100"
}
```

### 2. 聊天请求 (ChatRequest)
```json
{
  "timestamp": "2025-11-01T12:35:00.123456+00:00",
  "username": "admin",
  "action": {
    "chat_request": {
      "model": "deepseek-chat",
      "message_count": 5,
      "tokens_estimated": null
    }
  }
}
```

### 3. 配额检查 (QuotaCheck)
```json
{
  "timestamp": "2025-11-01T12:35:00.123456+00:00",
  "username": "admin",
  "action": {
    "quota_check": {
      "used": 45,
      "remaining": 455
    }
  }
}
```

### 4. 配额耗尽 (QuotaExceeded)
```json
{
  "timestamp": "2025-11-01T12:35:10.123456+00:00",
  "username": "user1",
  "action": {
    "quota_exceeded": {
      "used": 500,
      "limit": 500
    }
  }
}
```

### 5. 速率限制 (RateLimited)
```json
{
  "timestamp": "2025-11-01T12:35:15.123456+00:00",
  "username": "user1",
  "action": "rate_limited"
}
```

### 6. 错误 (Error)
```json
{
  "timestamp": "2025-11-01T12:35:20.123456+00:00",
  "username": "admin",
  "action": {
    "error": {
      "error_type": "upstream_timeout",
      "message": "上游服务响应超时"
    }
  }
}
```

## 日志分析示例

### 1. 查看用户今天的所有操作
```bash
cat logs/users/admin/admin.$(date +%Y-%m-%d).log | jq
```

### 2. 统计用户的请求次数
```bash
cat logs/users/admin/*.log | jq -r 'select(.action.chat_request) | .timestamp' | wc -l
```

### 3. 查找配额耗尽的记录
```bash
cat logs/users/*/*.log | jq 'select(.action.quota_exceeded)'
```

### 4. 查看所有错误
```bash
cat logs/users/admin/*.log | jq 'select(.action.error)'
```

### 5. 按日期统计请求量
```bash
for file in logs/users/admin/*.log; do
    echo "$file:"
    jq -r 'select(.action.chat_request)' "$file" | jq -s 'length'
done
```

### 6. 查看最近的 N 条日志
```bash
tail -n 50 logs/users/admin/admin.$(date +%Y-%m-%d).log | jq
```

## 存储空间估算

- **单个日志文件**: 最大 5 MB
- **每个用户保留**: 10 个文件
- **单用户最大占用**: 50 MB
- **100 个用户最大占用**: ~5 GB

## 隐私和安全

### 记录的信息
✅ 记录：
- 用户名
- 操作时间
- 操作类型
- 配额使用情况
- 错误信息

❌ 不记录：
- 密码
- API Key
- 聊天内容
- Token 内容

### 日志保护
- 日志文件只能由服务进程访问
- 建议设置适当的文件权限：`chmod 600 logs/users/*/*`
- 定期备份重要用户的日志

## 维护建议

### 定期清理
虽然有自动清理机制，但建议定期手动检查：

```bash
# 查看日志总大小
du -sh logs/users/

# 查看每个用户的日志大小
du -sh logs/users/*/

# 清理 30 天前的日志（如果需要）
find logs/users/ -name "*.log" -mtime +30 -delete
```

### 日志轮转配置
如果需要调整配置，修改 `src/user_activity.rs`：

```rust
Self {
    base_dir: base_dir.into(),
    max_file_size: 5 * 1024 * 1024, // 修改单文件大小限制
    file_handles: Arc::new(Mutex::new(HashMap::new())),
}
```

清理保留的文件数量：
```rust
const MAX_FILES: usize = 10; // 修改保留的文件数量
```

## 故障排查

### 日志未创建
1. 检查目录权限：`ls -la logs/users/`
2. 检查磁盘空间：`df -h`
3. 查看服务日志中的错误信息

### 日志文件过大
1. 检查是否有大量错误日志
2. 检查滚动机制是否正常工作
3. 考虑减小 `max_file_size`

### 性能影响
- 日志写入是异步的，对性能影响极小
- 如果担心性能，可以考虑使用非阻塞写入
- 定期清理可能会有短暂的 IO 峰值
