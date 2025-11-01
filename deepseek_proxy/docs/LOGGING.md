# 日志系统说明

## 功能特性

DeepSeek Proxy 使用了高性能的日志系统，具有以下特性：

### 1. 双重输出
- **控制台输出**: 人类可读的彩色日志，便于开发调试
- **文件输出**: JSON 格式的结构化日志，便于日志分析和查询

### 2. 自动滚动（Rotation）
- **按日期滚动**: 每天自动创建新的日志文件
- **按大小滚动**: 单个文件最大 10MB，超过后自动创建新文件
- **自动清理**: 只保留最近的 5 个日志文件

### 3. 非阻塞 IO
- 使用异步非阻塞写入，不会影响主服务性能
- 后台定时任务管理日志文件

## 日志文件位置

所有日志文件存储在 `logs/` 目录下：

```
logs/
├── deepseek_proxy.2025-11-01.log
├── deepseek_proxy.2025-11-02.log
├── deepseek_proxy.2025-11-03.log
└── ...
```

## 日志配置

在 `src/main.rs` 中可以修改日志配置：

```rust
logger::init_logger(logger::LoggerConfig {
    log_dir: "logs".to_string(),           // 日志目录
    file_prefix: "deepseek_proxy".to_string(), // 文件名前缀
    max_file_size: 10 * 1024 * 1024,       // 单文件最大 10MB
    max_files: 5,                           // 保留最近 5 个文件
})?;
```

## 日志级别

通过环境变量 `RUST_LOG` 控制日志级别：

```bash
# 开发环境（详细日志）
export RUST_LOG=deepseek_proxy=debug,tower_http=debug

# 生产环境（仅重要信息）
export RUST_LOG=deepseek_proxy=info,tower_http=warn

# 完全静默
export RUST_LOG=deepseek_proxy=error
```

## 日志格式

### 控制台输出示例
```
2025-11-01T12:34:56.789012+08:00  INFO deepseek_proxy: 配置加载成功
2025-11-01T12:34:56.790123+08:00  INFO deepseek_proxy: 服务器地址: 0.0.0.0:8877
```

### 文件输出示例（JSON）
```json
{
  "timestamp": "2025-11-01T12:34:56.789012+08:00",
  "level": "INFO",
  "target": "deepseek_proxy",
  "fields": {
    "message": "配置加载成功"
  },
  "span": {
    "name": "main"
  }
}
```

## 日志分析

使用 `jq` 工具分析 JSON 日志：

```bash
# 查看所有错误日志
cat logs/deepseek_proxy.*.log | jq 'select(.level=="ERROR")'

# 统计各级别日志数量
cat logs/deepseek_proxy.*.log | jq -r '.level' | sort | uniq -c

# 查找特定用户的操作
cat logs/deepseek_proxy.*.log | jq 'select(.fields.username=="admin")'

# 查看最近 100 条日志
tail -n 100 logs/deepseek_proxy.$(date +%Y-%m-%d).log | jq
```

## 清理日志

日志会自动清理，但如果需要手动清理：

```bash
# 清理 7 天前的日志
find logs/ -name "*.log" -mtime +7 -delete

# 清理所有日志
rm -rf logs/
```

## 注意事项

1. **磁盘空间**: 最多占用 50MB (5个文件 × 10MB)
2. **性能影响**: 非阻塞写入，对服务性能影响极小
3. **日志敏感信息**: 不要在日志中记录密码、API Key 等敏感信息
4. **时区**: 日志使用东八区时间 (UTC+8)

## 故障排查

### 日志文件未创建
- 检查目录权限：`ls -la logs/`
- 检查磁盘空间：`df -h`

### 日志文件过大
- 检查是否有大量错误日志
- 考虑降低日志级别到 `info` 或 `warn`

### 日志清理不工作
- 检查后台任务是否运行
- 查看控制台是否有错误信息
