use once_cell::sync::Lazy;
use prometheus::{Registry, Counter, CounterVec, Histogram, HistogramOpts, TextEncoder, Encoder, IntGauge};
use std::time::Instant;
use std::sync::Mutex;
use chrono::{Local};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use std::fs;
use anyhow::Result;

#[derive(Serialize, Deserialize)]
struct DailySnapshot {
    date: String,
    login_success: u64,
    login_fail: u64,
    login_bruteforce_blocked: u64,
    rate_limit_rejections: u64,
    chat_success: u64,
    chat_fail: u64,
    today_input_tokens: i64,
    today_output_tokens: i64,
    today_prompt_cache_hit_tokens: i64,
    today_prompt_cache_miss_tokens: i64,
    updated_at: String,
}

pub struct Metrics {
    pub registry: Registry,
    pub login_attempts: CounterVec,
    pub login_bruteforce_blocked: Counter,
    pub rate_limit_rejections: Counter,
    pub quota_status: CounterVec,
    pub upstream_latency: Histogram,
    pub upstream_errors: CounterVec,
    pub chat_requests: CounterVec,
    // 今日 token 消耗 (粗略估算) - input/output
    pub today_input_tokens: IntGauge,
    pub today_output_tokens: IntGauge,
    pub today_prompt_cache_hit_tokens: IntGauge,
    pub today_prompt_cache_miss_tokens: IntGauge,
    // 保存当前日期 (YYYY-MM-DD)，用于 rollover
    current_day: Mutex<String>,
    // 持久化目录（可后续做成配置，这里简单固定）
    persist_dir: PathBuf,
}

impl Metrics {
    fn new() -> Self {
        let registry = Registry::new();

        let login_attempts = CounterVec::new(
            prometheus::Opts::new("login_attempts_total", "Login attempts grouped by result"),
            &["result"],
        ).unwrap();
        registry.register(Box::new(login_attempts.clone())).unwrap();

        let login_bruteforce_blocked = Counter::new("login_bruteforce_blocked_total", "Blocked brute force logins").unwrap();
        registry.register(Box::new(login_bruteforce_blocked.clone())).unwrap();

        let rate_limit_rejections = Counter::new("rate_limit_rejections_total", "Requests rejected by rate limiter").unwrap();
        registry.register(Box::new(rate_limit_rejections.clone())).unwrap();

        let quota_status = CounterVec::new(
            prometheus::Opts::new("quota_checks_total", "Quota check results"),
            &["status"],
        ).unwrap();
        registry.register(Box::new(quota_status.clone())).unwrap();

        let upstream_latency = Histogram::with_opts(HistogramOpts::new(
            "upstream_latency_seconds",
            "Latency of upstream (DeepSeek) requests",
        ).buckets(vec![0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0])).unwrap();
        registry.register(Box::new(upstream_latency.clone())).unwrap();

        let upstream_errors = CounterVec::new(
            prometheus::Opts::new("upstream_errors_total", "Upstream errors grouped by kind"),
            &["kind"],
        ).unwrap();
        registry.register(Box::new(upstream_errors.clone())).unwrap();

        let chat_requests = CounterVec::new(
            prometheus::Opts::new("chat_requests_total", "Chat requests grouped by status"),
            &["status"],
        ).unwrap();
        registry.register(Box::new(chat_requests.clone())).unwrap();

        // 今日 input/output token 统计 (Gauge 可重置)
        let today_input_tokens = IntGauge::new("today_input_tokens", "Estimated input tokens consumed today").unwrap();
        registry.register(Box::new(today_input_tokens.clone())).unwrap();
    let today_output_tokens = IntGauge::new("today_output_tokens", "Estimated output tokens consumed today").unwrap();
    registry.register(Box::new(today_output_tokens.clone())).unwrap();
    let today_prompt_cache_hit_tokens = IntGauge::new("today_prompt_cache_hit_tokens", "Prompt cache HIT tokens today").unwrap();
    registry.register(Box::new(today_prompt_cache_hit_tokens.clone())).unwrap();
    let today_prompt_cache_miss_tokens = IntGauge::new("today_prompt_cache_miss_tokens", "Prompt cache MISS tokens today").unwrap();
    registry.register(Box::new(today_prompt_cache_miss_tokens.clone())).unwrap();

        let current_day = Mutex::new(Local::now().format("%Y-%m-%d").to_string());
        let persist_dir = PathBuf::from("data/metrics/daily");

        Self {
            registry,
            login_attempts,
            login_bruteforce_blocked,
            rate_limit_rejections,
            quota_status,
            upstream_latency,
            upstream_errors,
            chat_requests,
            today_input_tokens,
            today_output_tokens,
            today_prompt_cache_hit_tokens,
            today_prompt_cache_miss_tokens,
            current_day,
            persist_dir,
        }
    }

    pub fn render(&self) -> Result<String, String> {
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).map_err(|e| e.to_string())?;
        String::from_utf8(buffer).map_err(|e| e.to_string())
    }

    fn rollover_if_needed(&self) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let mut guard = self.current_day.lock().unwrap();
        if *guard != today {
            // 新的一天，重置 gauge
            self.today_input_tokens.set(0);
            self.today_output_tokens.set(0);
            self.today_prompt_cache_hit_tokens.set(0);
            self.today_prompt_cache_miss_tokens.set(0);
            *guard = today;
        }
    }

    pub fn record_input_tokens(&self, tokens: u32) {
        self.rollover_if_needed();
        if tokens > 0 {
            self.today_input_tokens.add(tokens as i64);
        }
    }

    pub fn record_output_tokens(&self, tokens: u32) {
        self.rollover_if_needed();
        if tokens > 0 {
            self.today_output_tokens.add(tokens as i64);
        }
    }

    pub fn record_prompt_cache_hit_tokens(&self, tokens: u32) {
        self.rollover_if_needed();
        if tokens > 0 { self.today_prompt_cache_hit_tokens.add(tokens as i64); }
    }

    pub fn record_prompt_cache_miss_tokens(&self, tokens: u32) {
        self.rollover_if_needed();
        if tokens > 0 { self.today_prompt_cache_miss_tokens.add(tokens as i64); }
    }

    // ===== 持久化实现（简化版：仅今日，启动加载 / 关闭保存） =====

    fn today_file_path(&self) -> PathBuf {
        let day = Local::now().format("%Y-%m-%d").to_string();
        self.persist_dir.join(format!("{}.json", day))
    }

    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.persist_dir)?;
        Ok(())
    }

    fn counter_value(&self, cv: &CounterVec, label: &[&str]) -> u64 {
        cv.get_metric_with_label_values(label).map(|m| m.get() as u64).unwrap_or(0)
    }
    fn counter_simple(&self, c: &Counter) -> u64 { c.get() as u64 }
    fn gauge_value(&self, g: &IntGauge) -> i64 { g.get() }

    fn build_snapshot(&self) -> DailySnapshot {
        DailySnapshot {
            date: Local::now().format("%Y-%m-%d").to_string(),
            login_success: self.counter_value(&self.login_attempts, &["success"]),
            login_fail: self.counter_value(&self.login_attempts, &["fail"]),
            login_bruteforce_blocked: self.counter_simple(&self.login_bruteforce_blocked),
            rate_limit_rejections: self.counter_simple(&self.rate_limit_rejections),
            chat_success: self.counter_value(&self.chat_requests, &["success"]),
            chat_fail: self.counter_value(&self.chat_requests, &["fail"]),
            today_input_tokens: self.gauge_value(&self.today_input_tokens),
            today_output_tokens: self.gauge_value(&self.today_output_tokens),
            today_prompt_cache_hit_tokens: self.gauge_value(&self.today_prompt_cache_hit_tokens),
            today_prompt_cache_miss_tokens: self.gauge_value(&self.today_prompt_cache_miss_tokens),
            updated_at: Local::now().to_rfc3339(),
        }
    }

    pub fn save_today(&self) -> Result<()> {
        self.ensure_dir()?;
        let path = self.today_file_path();
        let tmp = path.with_extension("json.tmp");
        let snapshot = self.build_snapshot();
        let json = serde_json::to_string_pretty(&snapshot)?;
        fs::write(&tmp, json)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn load_today(&self) -> Result<()> {
        self.ensure_dir()?;
        let path = self.today_file_path();
        if !path.exists() { return Ok(()); }
        let content = fs::read_to_string(&path)?;
        let snapshot: DailySnapshot = serde_json::from_str(&content)?;
        let today = Local::now().format("%Y-%m-%d").to_string();
        if snapshot.date != today {
            // 旧文件，不加载
            return Ok(());
        }
        // 将快照值恢复到当前指标：Counter 通过 inc_by，Gauge 通过 set
        let success_metric = self.login_attempts.get_metric_with_label_values(&["success"]).unwrap();
        let fail_metric = self.login_attempts.get_metric_with_label_values(&["fail"]).unwrap();
        let chat_success_metric = self.chat_requests.get_metric_with_label_values(&["success"]).unwrap();
        let chat_fail_metric = self.chat_requests.get_metric_with_label_values(&["fail"]).unwrap();

        // 当前值（启动时基本为0）
        let cur_login_success = success_metric.get();
        let cur_login_fail = fail_metric.get();
        let cur_bruteforce = self.login_bruteforce_blocked.get();
        let cur_rate_rej = self.rate_limit_rejections.get();
        let cur_chat_success = chat_success_metric.get();
        let cur_chat_fail = chat_fail_metric.get();

        if snapshot.login_success as f64 > cur_login_success { success_metric.inc_by(snapshot.login_success as f64 - cur_login_success); }
        if snapshot.login_fail as f64 > cur_login_fail { fail_metric.inc_by(snapshot.login_fail as f64 - cur_login_fail); }
        if snapshot.login_bruteforce_blocked as f64 > cur_bruteforce { self.login_bruteforce_blocked.inc_by(snapshot.login_bruteforce_blocked as f64 - cur_bruteforce); }
        if snapshot.rate_limit_rejections as f64 > cur_rate_rej { self.rate_limit_rejections.inc_by(snapshot.rate_limit_rejections as f64 - cur_rate_rej); }
        if snapshot.chat_success as f64 > cur_chat_success { chat_success_metric.inc_by(snapshot.chat_success as f64 - cur_chat_success); }
        if snapshot.chat_fail as f64 > cur_chat_fail { chat_fail_metric.inc_by(snapshot.chat_fail as f64 - cur_chat_fail); }

        self.today_input_tokens.set(snapshot.today_input_tokens);
        self.today_output_tokens.set(snapshot.today_output_tokens);
        self.today_prompt_cache_hit_tokens.set(snapshot.today_prompt_cache_hit_tokens);
        self.today_prompt_cache_miss_tokens.set(snapshot.today_prompt_cache_miss_tokens);

        Ok(())
    }

    pub fn cleanup_old_days(&self, keep_days: u32) -> Result<()> {
        self.ensure_dir()?;
        let entries = fs::read_dir(&self.persist_dir)?;
        let today = Local::now();
        for entry in entries {
            if let Ok(e) = entry {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
                if let Some(fname) = path.file_stem().and_then(|s| s.to_str()) {
                    // 解析日期
                    if let Ok(file_date) = chrono::NaiveDate::parse_from_str(fname, "%Y-%m-%d") {
                        let duration = today.date_naive() - file_date;
                        if duration.num_days() > keep_days as i64 {
                            let _ = fs::remove_file(&path); // 忽略错误
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub static METRICS: Lazy<Metrics> = Lazy::new(|| Metrics::new());

pub struct UpstreamTimer {
    start: Instant,
}
impl UpstreamTimer {
    pub fn start() -> Self { Self { start: Instant::now() } }
    pub fn observe(self) {
        let elapsed = self.start.elapsed();
        METRICS.upstream_latency.observe(elapsed.as_secs_f64());
    }
}
