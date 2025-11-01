# 观测性与安全设计说明

> 版本：2025-11-01
> 范围：当前实现的 Metrics、BruteForceGuard（登录暴力破解防护）、Webhook 事件、配置项、扩展建议。

---
## 1. 架构总览

```
          +-------------------+            +------------------+
 Request  |   Axum Router     |  /login    |  BruteForceGuard |
  ----->  |  (middleware链)   |----------->|  计数 + 阻断逻辑  |
           \        |         /            +------------------+
            \       |        /                    ^
             v      v       /metrics              |
          +-------------------+                   |
          |   Metrics 模块    |<------------------+
          | (prometheus client)| 失败/阻断事件 -> login_bruteforce_blocked 计数
          +---------+---------+
                    |
                    v
          +-------------------+
          |  Prometheus Pull  |
          | (/metrics 暴露)    |
          +-------------------+
                    |
                    v
          +-------------------+      可选
          |   AlertManager    |<----- Webhook 回调 (未来可改为主动推送)
          +-------------------+
```

- 登录请求在 `auth::handler::login` 中先调用 `BruteForceGuard.should_block`，被阻断会直接返回错误并递增对应指标。
- 上游调用、聊天、限流、配额等在关键路径埋入 Prometheus 指标。
- `/metrics` 暴露所有指标供 Prometheus 抓取。
- 暴力破解阻断事件可以通过 Webhook（占位实现）异步通知外部系统。

---
## 2. 指标（Metrics）清单

文件：`deepseek_proxy/src/metrics.rs`

| 名称 | 类型 | 标签 | 说明 | 更新位置示例 |
|------|------|------|------|--------------|
| `login_total` | Counter | `result` (success|failure) | 登录尝试次数 | `auth::handler::login` |
| `login_bruteforce_blocked` | Counter | 无 | 暴力破解阻断次数 | `auth::handler::login` 阻断分支 |
| `quota_check_total` | Counter | `status` (allow|deny) | 配额检查结果 | `proxy::handler` 在配额分支 |
| `rate_limit_reject_total` | Counter | 无 | 全局/用户限流拒绝次数 | `proxy::handler` 限流失败分支 |
| `chat_success_total` | Counter | 无 | 成功发起上游聊天（开始流）次数 | `proxy::handler` SSE 构建后 |
| `upstream_latency_seconds` | Histogram | 无 | 上游接口首包延迟 | `deepseek::client` timer.observe |
| `upstream_error_total` | Counter | `kind` (network|api) | 上游错误分类次数 | `deepseek::client` 错误分支 |

### 2.1 指标语义与使用建议
- 登录失败与暴力破解阻断之间的差异：阻断发生时通常已达到阈值，阻断次数急速上升可报警。
- 上游延迟：95/99 分位超过阈值（例如 2s）时可报警，指示第三方模型性能下降。
- 配额 deny 与 rate-limit reject：可用来区分业务额度耗尽与防护限流策略触发。

### 2.2 Prometheus 抓取配置示例
在 Prometheus `prometheus.yml`：
```yaml
scrape_configs:
  - job_name: 'deepseek_proxy'
    metrics_path: /metrics
    scheme: http
    static_configs:
      - targets: ['proxy-host:port']
```

### 2.3 常见报警规则示例 (Prometheus alerting rules)
```yaml
groups:
- name: deepseek_proxy.rules
  rules:
  - alert: BruteForceExploding
    expr: increase(login_bruteforce_blocked[5m]) > 20
    for: 1m
    labels:
      severity: warning
    annotations:
      summary: "登录暴力阻断频率异常"
  - alert: UpstreamHighLatencyP95
    expr: histogram_quantile(0.95, sum by (le) (rate(upstream_latency_seconds_bucket[5m]))) > 2
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "上游接口延迟过高 (P95 > 2s)"
  - alert: RateLimitStorm
    expr: increase(rate_limit_reject_total[1m]) > 100
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: "一分钟内限流拒绝过多"
```

---
## 3. BruteForceGuard 使用与配置

文件：`deepseek_proxy/src/auth/bruteforce.rs`

### 3.1 配置结构
在 `config.rs` 中新增：
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub brute_force_window_secs: u64,   // 统计窗口秒数
    pub brute_force_threshold: u32,     // 超过则阻断
    pub webhook_url: Option<String>,   // 阻断事件通知地址
}
```
默认值：窗口 60 秒，阈值 5。

可在 `config.toml` 中添加：
```toml
[security]
brute_force_window_secs = 60
brute_force_threshold = 5
webhook_url = "https://alert.example.com/hook"
```
支持环境变量覆盖（遵循现有加载模式）。

### 3.2 逻辑流程
1. 登录请求进入 handler，提取用户名与客户端 IP（`ConnectInfo<SocketAddr>`）。
2. 组合 key = `username:ip`。
3. 调用 `guard.should_block(&key, now)`：
   - 若已达到阈值且在窗口内，直接返回 429/403 样式错误（当前实现返回错误 JSON）。
4. 若未阻断，继续校验用户名与密码：失败时调用 `record_failure`；成功时调用 `reset_on_success` 清除失败计数。
5. 当阻断发生时：
   - 递增指标 `login_bruteforce_blocked`
   - 若配置了 webhook_url，异步 `spawn_webhook_notify` 发送阻断事件。

### 3.3 数据结构
内部使用 `DashMap<String, Vec<Instant>>` 存储指定 key 最近失败时间戳列表；每次查询会清理超出窗口的旧记录。

### 3.4 Webhook 占位格式
当前示例发送 JSON：
```json
{
  "event": "login_bruteforce_blocked",
  "username": "alice",
  "ip": "203.0.113.10:54321",
  "fail_count": 7
}
```
未来可扩展：签名头、重试机制、批量聚合推送。

### 3.5 安全建议
- 生产环境务必使用密码哈希（当前可能为明文/简单对比）。
- 对 Webhook 增加 HMAC-SHA256 签名：`X-Signature: hmac_sha256(secret, body)`。
- 增加 IP 黑名单自动提升封禁时长策略（指数退避）。
- 将阻断事件写入持久化日志便于审计（当前仅指标 + Webhook）。

---
## 4. 使用示例

### 4.1 查看指标
浏览器或 curl：
```
curl http://localhost:PORT/metrics
```
输出类似：
```
# HELP login_total Login attempts
# TYPE login_total counter
login_total{result="success"} 10
login_total{result="failure"} 3
# HELP login_bruteforce_blocked Brute force block events
login_bruteforce_blocked 1
...
```

### 4.2 模拟暴力破解
循环错误密码 6 次：应返回错误并第 6 次触发阻断。指标 `login_bruteforce_blocked` 增加，Webhook 发送。

### 4.3 调整阈值
临时降低阈值：`export SECURITY_BRUTE_FORCE_THRESHOLD=3`（Windows PowerShell 使用：`$env:SECURITY_BRUTE_FORCE_THRESHOLD=3`），重启服务即可生效。

---
## 5. 排障 (Troubleshooting)
| 现象 | 可能原因 | 解决建议 |
|------|---------|---------|
| 阻断不生效 | 未正确提取客户端 IP | 确认使用 `Router::into_make_service_with_connect_info::<SocketAddr>()` |
| 指标不出现 | 未访问相关路径或埋点遗漏 | 检查 `metrics.rs` 初始化及对应 counter 是否递增 |
| 指标抓取 404 | 路由未挂载或端口错误 | 检查 `main.rs` 中 `/metrics` 版本路径 |
| 延迟分位异常低 | Histogram bucket 配置不匹配延迟范围 | 调整 `histogram_opts` bucket 列表 |
| Webhook 未触发 | 未配置 webhook_url 或网络拒绝 | 查看日志与抓包，再加重试机制 |

---
## 6. 扩展路线图
- 指标：增加 per-user / per-IP 限流拒绝标签（注意基数控制）。
- 安全：集成 geo/IP 信誉库（如 AbuseIPDB）自动提高风险评分。
- 告警：阻断频次与登录失败比值动态阈值（异常检测）。
- 日志：统一 LogSink 后为安全事件单独通道（JSON lines + SIEM）。
- 上游：错误分类细化（超时、HTTP 状态、解析失败）。

---
## 7. 与其它模块的集成点
| 模块 | 交互 | 风险点 |
|------|------|--------|
| `auth::handler` | 首次入口调用阻断判断与记录失败 | 不要在成功登录后忘记 reset，避免误阻断 |
| `deepseek::client` | 延迟 Histogram 与错误 Counter | 注意异步超时要分类为 network 错误 |
| `proxy::handler` | 限流与配额 counters | 添加标签时控制基数，避免爆炸 |
| `config.rs` | 加载 SecurityConfig | 环境变量命名与 toml 字段需保持一致 |

---
## 8. 快速检查清单
- [ ] `/metrics` 返回 200 且含 `login_total`。
- [ ] 连续失败达到阈值后阻断响应出现。
- [ ] `login_bruteforce_blocked` 数值 > 0。
- [ ] 配置修改后重启生效（确认阈值变动）。
- [ ] 上游调用产生 `upstream_latency_seconds_bucket`。
- [ ] 无多余编译警告。

---
## 9. FAQ
**Q: 为什么不用 push gateway?** 目前服务保持拉取模型，避免 push gateway 额外复杂度。需要批量短生命周期任务再考虑。

**Q: 指标数量还能增加吗?** 可以，但需关注高基数标签的内存占用；优先聚合维度（结果/状态），慎用用户级标签。

**Q: Webhook 失败会怎么办?** 现阶段简单 fire-and-forget；后续可加入有界重试队列 + 指标失败统计。

---
## 10. 参考
- Prometheus 官方文档: https://prometheus.io/docs/introduction/overview/
- DashMap 性能讨论: https://crates.io/crates/dashmap
- 暴力破解防护最佳实践：基于窗口计数 + 渐进式封禁策略。

---
(完)
