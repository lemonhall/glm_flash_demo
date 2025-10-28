# 配额计费系统设计文档

## 一、核心目标

基于**安全**和**计费**两个核心需求，实现简单可靠的月度配额管理系统。

---

## 二、配额档次设计

### 2.1 配额分级

| 档次 | 名称 | 月配额 | 适用场景 |
|------|------|--------|----------|
| `basic` | 基础版 | 500次/月 | 个人试用 |
| `pro` | 专业版 | 1000次/月 | 小团队 |
| `premium` | 高级版 | 1500次/月 | 企业用户 |

### 2.2 配置文件

```toml
# config.toml
[quota]
basic = 500
pro = 1000
premium = 1500
monthly_reset_day = 1  # 每月1号0点重置

[[auth.users]]
username = "user_basic"
password = "pass123"
quota_tier = "basic"      # 配额档次
is_active = true          # 账号是否激活

[[auth.users]]
username = "user_pro"
password = "pass456"
quota_tier = "pro"

[[auth.users]]
username = "user_premium"
password = "pass789"
quota_tier = "premium"
```

---

## 三、数据结构设计

### 3.1 核心数据结构

```rust
/// 用户配额信息
pub struct UserQuota {
    pub username: String,
    pub tier: QuotaTier,           // 配额档次
    pub monthly_limit: u32,        // 月配额上限
    pub used_count: u32,           // 本月已使用次数
    pub reset_at: DateTime<Utc>,  // 下次重置时间
    pub is_active: bool,           // 账号是否激活
    pub updated_at: DateTime<Utc>, // 最后更新时间
}

/// 配额档次枚举
#[derive(Debug, Clone, Copy)]
pub enum QuotaTier {
    Basic,      // 500次/月
    Pro,        // 1000次/月
    Premium,    // 1500次/月
}

impl QuotaTier {
    pub fn limit(&self) -> u32 {
        match self {
            QuotaTier::Basic => 500,
            QuotaTier::Pro => 1000,
            QuotaTier::Premium => 1500,
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "basic" => Some(QuotaTier::Basic),
            "pro" => Some(QuotaTier::Pro),
            "premium" => Some(QuotaTier::Premium),
            _ => None,
        }
    }
}

/// 配额检查结果
#[derive(Debug)]
pub enum QuotaStatus {
    /// 配额充足，可以继续请求
    Ok {
        used: u32,           // 本次请求后的使用次数
        limit: u32,          // 配额上限
        remaining: u32,      // 剩余次数
        reset_at: DateTime<Utc>,  // 重置时间
    },
    /// 配额已耗尽，需要付费
    Exceeded {
        used: u32,           // 已使用次数
        limit: u32,          // 配额上限
        reset_at: DateTime<Utc>,  // 重置时间
    },
    /// 账号未激活或已禁用
    AccountDisabled,
}
```

---

## 四、HTTP 状态码设计

| 状态码 | 名称 | 含义 | 客户端行为 |
|--------|------|------|-----------|
| **200** | OK | 请求成功 | 正常处理响应 |
| **401** | Unauthorized | Token 无效/过期 | 重新登录 |
| **402** | Payment Required | **配额耗尽，需要付费** | 提示升级套餐或等待下月重置 |
| **403** | Forbidden | 账号已禁用/未激活 | 联系管理员 |
| **429** | Too Many Requests | 同一 Token 并发限制 | 等待当前请求完成 |
| **502** | Bad Gateway | DeepSeek API 错误 | 稍后重试 |
| **504** | Gateway Timeout | DeepSeek API 超时 | 稍后重试 |

### 4.1 402 响应格式

```json
{
    "error": "quota_exceeded",
    "message": "Monthly quota exceeded. Please upgrade your plan.",
    "details": {
        "used": 500,
        "limit": 500,
        "reset_at": "2025-11-01T00:00:00Z"
    },
    "upgrade_url": "https://your-site.com/upgrade"
}
```

---

## 五、核心接口设计

### 5.1 配额检查接口（内部）

```rust
pub trait QuotaChecker {
    /// 检查并递增配额计数
    /// 返回 QuotaStatus 表示配额状态
    async fn check_and_increment(&self, username: &str) -> Result<QuotaStatus>;
    
    /// 查询配额信息（不递增）
    async fn get_quota(&self, username: &str) -> Result<Option<UserQuota>>;
    
    /// 重置用户配额（每月1号自动调用或手动调用）
    async fn reset_quota(&self, username: &str) -> Result<()>;
}
```

### 5.2 配额查询接口（HTTP API）

```
GET /auth/quota
Authorization: Bearer {token}

Response 200:
{
    "username": "user_basic",
    "tier": "basic",
    "monthly_limit": 500,
    "used_count": 320,
    "remaining": 180,
    "reset_at": "2025-11-01T00:00:00Z",
    "usage_percentage": 64.0,
    "is_active": true
}
```

### 5.3 使用历史接口（可选）

```
GET /auth/usage?start_date=2025-10-01&end_date=2025-10-31
Authorization: Bearer {token}

Response 200:
{
    "username": "user_basic",
    "period": {
        "start": "2025-10-01",
        "end": "2025-10-31"
    },
    "total_requests": 320,
    "daily_breakdown": [
        {"date": "2025-10-01", "requests": 15},
        {"date": "2025-10-02", "requests": 20},
        ...
    ]
}
```

---

## 六、处理流程设计

### 6.1 请求处理流程

```
客户端请求
    ↓
1. JWT Token 验证
    ↓ (无效)
    ├──→ 401 Unauthorized
    ↓ (有效)
2. 检查账号状态 (is_active)
    ↓ (禁用)
    ├──→ 403 Forbidden
    ↓ (激活)
3. 检查配额 (check_and_increment)
    ↓
    ├─ 检查是否需要月度重置
    │   └─ 如果当前时间 > reset_at，重置 used_count = 0
    ↓
    ├─ 检查配额是否充足
    │   └─ 如果 used_count >= monthly_limit
    │       └──→ 402 Payment Required
    ↓
    ├─ 递增计数: used_count += 1
    ↓
4. Token 串行化检查
    ↓ (已有请求)
    ├──→ 429 Too Many Requests
    ↓ (无请求)
5. 转发到 DeepSeek API
    ↓
6. 返回流式响应
```

### 6.2 月度重置逻辑

```rust
// 计算下个月1号 0点（UTC）
fn next_month_reset() -> DateTime<Utc> {
    let now = Utc::now();
    let next_month = if now.month() == 12 {
        NaiveDate::from_ymd_opt(now.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1).unwrap()
    };
    DateTime::from_naive_utc_and_offset(
        next_month.and_hms_opt(0, 0, 0).unwrap(),
        Utc
    )
}

// 检查并自动重置
async fn check_and_increment(username: &str) -> QuotaStatus {
    let now = Utc::now();
    let quota = get_quota(username).await;
    
    // 自动重置
    if now > quota.reset_at {
        quota.used_count = 0;
        quota.reset_at = next_month_reset();
        save_quota(&quota).await;
    }
    
    // 检查配额
    if quota.used_count >= quota.monthly_limit {
        return QuotaStatus::Exceeded {
            used: quota.used_count,
            limit: quota.monthly_limit,
            reset_at: quota.reset_at,
        };
    }
    
    // 递增计数
    quota.used_count += 1;
    save_quota(&quota).await;
    
    QuotaStatus::Ok {
        used: quota.used_count,
        limit: quota.monthly_limit,
        remaining: quota.monthly_limit - quota.used_count,
        reset_at: quota.reset_at,
    }
}
```

---

## 七、安全增强建议

### 7.1 IP 限流（防刷）

```rust
/// 每个 IP 每分钟最多 10 次请求
pub struct IpRateLimiter {
    // ip -> (count, window_start)
    limits: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    max_requests: u32,  // 10
    window: Duration,   // 60 seconds
}
```

### 7.2 登录频率限制

```rust
/// 每个用户每分钟最多登录 3 次
pub struct LoginRateLimiter {
    // username -> (count, window_start)
    attempts: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    max_attempts: u32,  // 3
    window: Duration,   // 60 seconds
}
```

### 7.3 审计日志

```rust
/// 审计日志记录
pub struct AuditLog {
    pub timestamp: DateTime<Utc>,
    pub username: String,
    pub action: String,         // "login", "chat", "quota_check"
    pub ip: String,
    pub user_agent: String,
    pub status_code: u16,
    pub quota_used: Option<u32>,
    pub quota_remaining: Option<u32>,
}

// 日志格式示例
2025-10-28 13:30:45 | user_basic | chat | 192.168.1.100 | 200 | used:321/500
2025-10-28 13:31:20 | user_basic | chat | 192.168.1.100 | 402 | quota_exceeded
2025-10-28 13:32:15 | user_pro   | login| 192.168.1.101 | 200 | -
```

### 7.4 配额预警响应头

```rust
// 当使用达到 80% 时，在响应头中添加警告
if used_count >= monthly_limit * 0.8 {
    response.headers.insert(
        "X-Quota-Warning", 
        format!("{}% used", (used_count * 100 / monthly_limit))
    );
    response.headers.insert(
        "X-Quota-Remaining",
        (monthly_limit - used_count).to_string()
    );
    response.headers.insert(
        "X-Quota-Reset",
        reset_at.to_rfc3339()
    );
}
```

---

## 八、错误处理设计

### 8.1 错误类型定义

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),  // 401
    
    #[error("Payment required: quota exceeded")]
    PaymentRequired {      // 402
        used: u32,
        limit: u32,
        reset_at: String,
    },
    
    #[error("Forbidden: account disabled")]
    AccountDisabled,       // 403
    
    #[error("Too many requests")]
    TooManyRequests,       // 429
    
    #[error("Internal error: {0}")]
    InternalError(String), // 500
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::PaymentRequired { used, limit, reset_at } => (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({
                    "error": "quota_exceeded",
                    "message": "Monthly quota exceeded. Please upgrade your plan.",
                    "details": {
                        "used": used,
                        "limit": limit,
                        "reset_at": reset_at
                    },
                    "upgrade_url": "https://your-site.com/upgrade"
                }))
            ).into_response(),
            
            Self::AccountDisabled => (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "account_disabled",
                    "message": "Your account has been disabled. Please contact support."
                }))
            ).into_response(),
            
            // ... 其他错误
        }
    }
}
```

---

## 九、测试用例设计

### 9.1 配额正常使用

```python
# 用户在配额内正常请求
token = login("user_basic", "pass123")

for i in range(10):
    response = chat(token, f"测试请求 {i}")
    assert response.status_code == 200
    
# 查询配额
quota = get_quota(token)
assert quota["used_count"] == 10
assert quota["remaining"] == 490
```

### 9.2 配额耗尽测试

```python
token = login("user_basic", "pass123")

# 模拟使用掉所有配额
for i in range(500):
    response = chat(token, f"请求 {i}")
    if i < 499:
        assert response.status_code == 200
    else:
        # 第 500 次请求成功，第 501 次应该返回 402
        pass

# 第 501 次请求
response = chat(token, "超额请求")
assert response.status_code == 402
assert response.json()["error"] == "quota_exceeded"
```

### 9.3 月度重置测试

```python
# 修改系统时间到下个月（或等待实际时间）
# 或手动调用 reset_quota API

response = chat(token, "重置后的请求")
assert response.status_code == 200

quota = get_quota(token)
assert quota["used_count"] == 1  # 重置后从 1 开始
```

### 9.4 不同档次并发测试

```python
token_basic = login("user_basic", "pass123")
token_pro = login("user_pro", "pass456")
token_premium = login("user_premium", "pass789")

# 三个用户同时请求
results = ThreadPoolExecutor().map([
    lambda: chat(token_basic, "基础用户"),
    lambda: chat(token_pro, "专业用户"),
    lambda: chat(token_premium, "高级用户"),
])

# 都应该成功
for result in results:
    assert result.status_code == 200
```

---

## 十、实现优先级

### Phase 1: 核心计费功能（必须）
1. ✅ 配额档次定义（config.toml）
2. ✅ QuotaTier 枚举和 UserQuota 结构体
3. ✅ check_and_increment 核心逻辑
4. ✅ 402 错误处理和响应格式
5. ✅ 配额查询 API（GET /auth/quota）
6. ✅ 月度自动重置逻辑

### Phase 2: 安全增强（推荐）
7. ✅ IP 限流器（防刷接口）
8. ✅ 登录频率限制
9. ✅ 审计日志（记录到文件）
10. ✅ 配额预警响应头

### Phase 3: 高级功能（可选）
11. ⭐ 使用历史统计 API
12. ⭐ 管理员接口（手动调整配额）
13. ⭐ 配额购买/升级接口
14. ⭐ 邮件/Webhook 通知（配额预警、耗尽）

---

## 十一、持久化方案：懒加载 + 懒写入

### 核心设计理念

**目标**: 最小化磁盘 IO，保持系统简单高效

**策略**:
1. **懒加载（Lazy Loading）**: 用户首次请求时才从磁盘加载数据
2. **懒写入（Lazy Writing）**: 每 100 次请求写一次磁盘
3. **优雅关闭（Graceful Shutdown）**: Ctrl+C 时保存所有脏数据
4. **关键节点立即写入**: 月度重置时立即写盘

### 方案详细设计

#### 1. 数据存储格式

**每个用户一个 JSON 文件**:
```
data/quotas/
├── user_basic.json
├── user_pro.json
└── user_premium.json
```

**文件内容示例**:
```json
{
    "username": "user_basic",
    "tier": "basic",
    "monthly_limit": 500,
    "used_count": 320,
    "last_saved_count": 300,
    "reset_at": "2025-11-01T00:00:00Z",
    "last_saved_at": "2025-10-28T14:30:00Z"
}
```

#### 2. 核心数据结构

```rust
pub struct QuotaManager {
    /// 内存缓存: username -> QuotaState
    cache: Arc<Mutex<HashMap<String, QuotaState>>>,
    
    /// 配置
    config: Arc<Config>,
    
    /// 数据目录
    data_dir: PathBuf,
    
    /// 写入间隔（每N次请求写一次）
    save_interval: u32,  // 默认 100
}

#[derive(Serialize, Deserialize, Clone)]
struct QuotaState {
    username: String,
    tier: String,
    monthly_limit: u32,
    used_count: u32,           // 当前计数
    last_saved_count: u32,     // 上次保存时的计数
    reset_at: String,          // ISO 8601 格式
    
    #[serde(skip)]
    dirty: bool,               // 是否有未保存的修改
}
```

#### 3. 核心流程

**懒加载流程**:
```
用户请求到达
    ↓
检查内存缓存
    ├─ 命中 → 直接使用
    └─ 未命中 ↓
检查磁盘文件 (data/quotas/{username}.json)
    ├─ 存在 → 读取并加载到内存
    └─ 不存在 → 从 config.toml 初始化新用户
```

**懒写入流程**:
```
递增计数: used_count += 1
    ↓
检查是否达到保存间隔
    ├─ (used_count - last_saved_count) >= 100
    │   ├─ 写入磁盘
    │   ├─ 更新 last_saved_count = used_count
    │   └─ dirty = false
    └─ < 100
        └─ dirty = true (仅标记，不写盘)
```

**月度重置流程**:
```
检查时间: now > reset_at
    ↓
重置计数
    ├─ used_count = 0
    ├─ last_saved_count = 0
    ├─ reset_at = next_month_reset()
    └─ ⚠️ 立即写盘（不等100次）
```

**优雅关闭流程**:
```
Ctrl+C 信号到达
    ↓
遍历所有缓存的用户
    ├─ 如果 dirty == true
    │   └─ 写入磁盘
    └─ dirty == false
        └─ 跳过
    ↓
输出: "✅ 数据已保存"
    ↓
退出程序
```

#### 4. 数据丢失风险评估

| 场景 | 数据丢失情况 | 风险等级 |
|------|------------|----------|
| **正常关闭** (Ctrl+C) | 0 次 | ✅ 无风险 |
| **月度重置后** | 0 次 | ✅ 无风险 |
| **刚写完磁盘** | 0 次 | ✅ 无风险 |
| **异常崩溃** | 最多 99 次，平均 50 次 | ⚠️ 可接受 |

**风险分析**:
- 最坏情况: 丢失 99 次请求（500次配额的 19.8%）
- 平均情况: 丢失 50 次请求（500次配额的 10%）
- 对于配额慷慨的服务，这个损失完全可接受
- 可视为"免费赠送"给用户的容错额度

#### 5. 并发安全保证

**单机部署**:
- 使用 `Arc<Mutex<HashMap>>` 保证线程安全
- 每个用户独立文件，写入时无需全局锁

**文件写入流程**:
```rust
async fn save_one(&self, username: &str, state: &QuotaState) -> Result<()> {
    let file_path = self.data_dir.join(format!("{}.json", username));
    
    // 原子写入：先写临时文件，再重命名
    let temp_path = file_path.with_extension(".tmp");
    let json = serde_json::to_string_pretty(state)?;
    fs::write(&temp_path, json)?;
    fs::rename(temp_path, file_path)?;  // 原子操作
    
    Ok(())
}
```

#### 6. 性能优势

**传统方案（每次都写）**:
```
1000 次请求 = 1000 次磁盘写入
平均响应时间: +5ms（磁盘 IO）
```

**懒写入方案**:
```
1000 次请求 = 10 次磁盘写入（减少 99%）
平均响应时间: +0.05ms（几乎无感知）
```

### 方案对比总结

| 方案 | 磁盘 IO | 数据丢失风险 | 实现复杂度 | 适用场景 |
|------|---------|-------------|-----------|----------|
| **懒加载 + 懒写入** | 极低（99% ↓） | 低（平均50次） | 简单 | ✅ **推荐** |
| SQLite | 中等 | 无 | 中等 | 需要复杂查询 |
| Redis | 低 | 低 | 中等 | 分布式部署 |
| 每次都写 | 极高 | 无 | 简单 | 请求量极少 |

---

## 十二、目录结构

```
deepseek_proxy/
├── src/
│   ├── quota/
│   │   ├── mod.rs           # 模块导出
│   │   ├── types.rs         # QuotaTier, QuotaStatus 定义
│   │   └── manager.rs       # QuotaManager 核心实现
│   ├── auth/
│   │   ├── handler.rs       # login + get_quota API
│   │   └── ...
│   ├── proxy/
│   │   ├── handler.rs       # 集成 quota check
│   │   └── ...
│   └── main.rs              # 集成优雅关闭
├── config.toml              # 配额档次定义
└── data/
    └── quotas/              # 配额数据目录
        ├── user_basic.json
        ├── user_pro.json
        └── user_premium.json
```

### 配置文件更新

```toml
# config.toml
[quota]
save_interval = 100          # 每100次请求写一次磁盘
monthly_reset_day = 1        # 每月1号重置

[quota.tiers]
basic = 500
pro = 1000
premium = 1500

[[auth.users]]
username = "user_basic"
password = "pass123"
quota_tier = "basic"
is_active = true

[[auth.users]]
username = "user_pro"
password = "pass456"
quota_tier = "pro"
is_active = true

[[auth.users]]
username = "user_premium"
password = "pass789"
quota_tier = "premium"
is_active = true
```

---

## 十三、实现优先级

### Phase 1: 核心配额功能（优先实现）

**目标**: 实现基础的配额检查和计数功能

**任务清单**:
1. ✅ 创建 `src/quota/` 模块
2. ✅ 实现 `QuotaTier` 枚举和 `QuotaStatus` 枚举
3. ✅ 实现 `QuotaManager` 核心逻辑
   - 懒加载用户数据
   - check_and_increment 方法
   - 每 100 次写盘逻辑
   - 月度重置逻辑
4. ✅ 在 `config.toml` 中添加配额配置
5. ✅ 集成到 `AppState`
6. ✅ 在 `proxy/handler.rs` 中调用配额检查
7. ✅ 实现 402 错误处理
8. ✅ 实现优雅关闭（Ctrl+C 保存数据）

**预计时间**: 1 小时

---

### Phase 2: 配额查询接口（次要）

**目标**: 让用户可以查询自己的配额使用情况

**任务清单**:
1. ✅ 实现 `GET /auth/quota` 接口
2. ✅ 返回配额详情（已用、剩余、重置时间等）
3. ✅ 在响应头添加配额预警（X-Quota-Warning）

**预计时间**: 30 分钟

---

### Phase 3: 测试验证（必须）

**目标**: 确保功能正确性

**任务清单**:
1. ✅ 测试配额正常使用
2. ✅ 测试配额耗尽（402 响应）
3. ✅ 测试月度重置
4. ✅ 测试优雅关闭（数据保存）
5. ✅ 测试异常重启（数据恢复）
6. ✅ 更新 `test_proxy.py` 添加配额测试

**预计时间**: 30 分钟

---

### Phase 4: 安全增强（可选）

**目标**: 防止滥用

**任务清单**:
1. ⭐ IP 限流（每分钟 10 次）
2. ⭐ 登录频率限制（每分钟 3 次）
3. ⭐ 审计日志（记录所有请求）

**预计时间**: 1 小时

---

## 十四、立即开始实现

**选择**: Phase 1 - 核心配额功能

**理由**:
1. 核心功能，必须先实现
2. 包含完整的持久化方案（懒加载 + 懒写入）
3. 包含优雅关闭，保证数据安全
4. 实现后即可投入使用

**开始实现** ⏱️
