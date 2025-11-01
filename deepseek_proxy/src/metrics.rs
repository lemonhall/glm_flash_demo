use once_cell::sync::Lazy;
use prometheus::{Registry, Counter, CounterVec, Histogram, HistogramOpts, TextEncoder, Encoder};
use std::time::Instant;

pub struct Metrics {
    pub registry: Registry,
    pub login_attempts: CounterVec,
    pub login_bruteforce_blocked: Counter,
    pub rate_limit_rejections: Counter,
    pub quota_status: CounterVec,
    pub upstream_latency: Histogram,
    pub upstream_errors: CounterVec,
    pub chat_requests: CounterVec,
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

        Self {
            registry,
            login_attempts,
            login_bruteforce_blocked,
            rate_limit_rejections,
            quota_status,
            upstream_latency,
            upstream_errors,
            chat_requests,
        }
    }

    pub fn render(&self) -> Result<String, String> {
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).map_err(|e| e.to_string())?;
        String::from_utf8(buffer).map_err(|e| e.to_string())
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
