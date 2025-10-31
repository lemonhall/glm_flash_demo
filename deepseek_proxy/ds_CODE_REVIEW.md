# DeepSeek Proxy 代码审查报告

## 功能性 Bug

### 1. 认证与授权模块
- **文件**: `src/auth/jwt.rs`
  - Token 验证未检查过期时间：`validate_token` 方法使用 `Validation::default()`，默认不验证 `exp` 字段
  - 建议：设置 `validation.validate_exp = true`

- **文件**: `src/auth/user_manager.rs`
  - 密码明文存储：`find_user` 方法直接比较明文密码，存在安全风险
  - 建议：使用 bcrypt 或 Argon2 存储密码哈希值

- **文件**: `src/auth/handler.rs`
  - Token 有效期信息不一致：响应中的 `expires_in` 使用配置值，但实际缓存时间为 60 秒
  - 建议：统一使用缓存时间（60秒）作为响应值

### 2. 配额管理模块
- **文件**: `src/quota/manager.rs`
  - 月度重置逻辑竞态条件：检查重置条件和执行重置操作非原子性，可能导致多次重置
  - 建议：在锁内完成整个重置操作
  - 重置时间计算不准确：`next_month_reset` 使用 UTC 时间计算，未考虑东八区偏移
  - 建议：使用东八区本地时间计算下月1日

### 3. 代理处理模块
- **文件**: `src/proxy/handler.rs`
  - 配额扣除时机不当：在 API 请求成功后扣除配额，若服务崩溃会导致配额未扣除
  - 建议：在获取 API 响应前扣除配额，失败时回滚

- **文件**: `src/proxy/limiter.rs`
  - Token 缓存可能重复：`get_or_generate` 生成新 token 时未检查缓存更新
  - 建议：在插入缓存前再次检查用户名是否存在

### 4. 错误处理
- **文件**: `src/error.rs`
  - `PaymentRequired` 响应硬编码升级链接：值为 `"https://your-site.com/upgrade"`
  - 建议：从配置读取升级链接或允许动态设置

## 并发 Bug

### 1. 共享状态访问
- **文件**: `src/auth/user_manager.rs`
  - 文件 IO 操作持有锁：`save_user` 在持有写锁时执行文件写入
  - 建议：克隆数据后立即释放锁，再执行 IO

- **文件**: `src/proxy/limiter.rs`
  - 缓存清理效率低：每次访问执行全量 `retain`，大缓存可能阻塞
  - 建议：使用 TTL 缓存或定期清理任务

### 2. 配额管理
- **文件**: `src/quota/manager.rs`
  - 并发扣费计数不准确：`increment_quota` 未使用原子操作或细粒度锁
  - 建议：使用 `AtomicU32` 或 per-user 锁
  - 重置操作非原子：检查重置条件和执行重置分离
  - 建议：在单一锁内完成检查和重置

### 3. 信号量管理
- **文件**: `src/proxy/limiter.rs`
  - `get_token_and_permit` 非原子：生成 token 和获取信号量分离
  - 建议：在锁内完成 token 生成和信号量获取

## 设计缺陷

### 1. 架构设计
- **文件**: `src/main.rs`
  - `AppState` 过于庞大：包含太多组件，违反单一职责
  - 建议：拆分状态为多个专注的上下文

- **文件**: 全局
  - 错误消息硬编码中文：多处直接使用中文字符串
  - 建议：实现国际化支持或配置化错误消息

### 2. 安全性
- **文件**: `src/auth/`
  - JWT 使用 HS256：默认算法强度不足
  - 建议：升级至 HS512 或 RS256
  - 无密钥轮换机制：JWT 密钥静态配置
  - 建议：实现动态密钥轮换

### 3. 可维护性
- **文件**: `src/config.rs`
  - 配置验证缺失：如 `monthly_reset_day` 可设置为无效值（如 32）
  - 建议：添加配置验证逻辑

- **文件**: `src/deepseek/client.rs`
  - API 错误处理笼统：所有错误转换为 `GlmError`
  - 建议：定义详细的 API 错误类型

### 4. 性能问题
- **文件**: 全局
  - 多处使用全局 `Mutex`：如配额缓存、用户管理
  - 建议：使用 `RwLock` 或分片锁减少争用
  - 缺乏指标监控：无请求延迟、错误率等指标
  - 建议：集成 Prometheus 指标

### 5. 数据存储
- **文件**: `src/auth/user_manager.rs`
  - 密码明文存储：TOML 文件包含明文密码
  - 建议：存储密码哈希值
  - 无原子写入：直接覆盖用户文件
  - 建议：使用临时文件+重命名机制

- **文件**: `src/quota/manager.rs`
  - JSON 和 TOML 混用：配额用 JSON，用户配置用 TOML
  - 建议：统一使用 TOML 格式

## 总结建议
1. **安全加固**：实现密码哈希存储，升级 JWT 算法，添加密钥轮换
2. **并发优化**：使用细粒度锁或原子操作，避免 IO 操作持有锁
3. **错误处理**：统一错误消息源，实现详细错误分类
4. **性能提升**：添加指标监控，优化锁争用，实现请求重试
5. **代码维护**：拆分庞大模块，统一配置验证，完善文档
