use prometheus::{Gauge, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry};
use tracing::info;
use std::sync::Arc;
use tokio::sync::OnceCell;





// Declare the static OnceCell to hold the Metrics.
static METRICS_INSTANCE: OnceCell<Arc<Metrics>> = OnceCell::const_new();

/// Asynchronously initializes and gets a reference to the static `TokenCache`.
pub async fn get_metrics() -> &'static Arc<Metrics> {
    METRICS_INSTANCE.get_or_init(|| async { 
        info!("Initializing Metrics ...");
        Metrics::new()}
    ).await
}


#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,

    // Source metrics
    pub source_fetch_requests: IntCounterVec,
    pub source_fetch_failures: IntCounterVec,
    pub source_fetch_duration: HistogramVec,

    // Parser metrics
    pub parse_failures: IntCounter,
    // pub template_failures: IntCounterVec,

    // Cache metrics
    pub cached_tokens: IntGaugeVec,
    pub token_expiry_unix: IntGaugeVec,

    // Sink metrics
    pub sink_propagations: IntCounterVec,
    pub sink_failures: IntCounterVec,
    pub sink_duration: HistogramVec,

    // Config/runtime
    pub config_validation_errors: IntCounter,
    // pub config_reloads: IntCounterVec,
    pub up: IntGauge,

        // === Service resource metrics ===
    pub process_cpu_usage: Gauge,
    pub process_memory_usage: IntGauge,
    pub process_virtual_memory: IntGauge,
    pub process_open_fds: IntGauge,
    pub process_threads: IntGauge,
    pub process_start_time: IntGauge,
    pub process_uptime: IntGauge,
}

impl Metrics {
    fn new() -> Arc<Self> {
        let registry = Registry::new_custom(Some("tokenagent".into()), None).unwrap();

        let metrics: Arc<Metrics> = Arc::new(Self {
            // Source
            source_fetch_requests: IntCounterVec::new(Opts::new("source_fetch_requests_total","Total fetch attempts by source",),&["source", "source_type", "method"],).unwrap(),
            source_fetch_failures: IntCounterVec::new(Opts::new("source_fetch_failures_total", "Fetch failures by reason"),&["source", "reason"],).unwrap(),
            source_fetch_duration: HistogramVec::new(HistogramOpts::new("source_fetch_duration_seconds", "Fetch duration seconds").buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]),&["source"],).unwrap(),

            parse_failures: IntCounter::new("parse_extraction_failures_total","Parser/extraction failures",).unwrap(),

            // Cache
            cached_tokens: IntGaugeVec::new(Opts::new("cached_tokens_total", "Cached tokens per source"),&["source"],).unwrap(),
            token_expiry_unix: IntGaugeVec::new(Opts::new("token_expiry_unix_seconds", "Token expiry timestamp"),&["source", "token_id"],).unwrap(),

            // Sink
            sink_propagations: IntCounterVec::new(Opts::new("sink_propagations_total", "Total propagations"),&["sink", "sink_type", "source", "token_id"],).unwrap(),
            sink_failures: IntCounterVec::new(Opts::new("sink_failures_total", "Sink failures"),&["sink", "reason"],).unwrap(),
            sink_duration: HistogramVec::new(HistogramOpts::new("sink_propagation_duration_seconds", "Sink propagation time").buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),&["sink"],).unwrap(),

            // Config/runtime
            config_validation_errors: IntCounter::new("config_validation_errors_total","Validation errors during startup/config reload",).unwrap(),
            up: IntGauge::new("up", "1 if service is healthy").unwrap(),
            process_cpu_usage: Gauge::new("process_cpu_usage_percent", "CPU usage % of this process").unwrap(),
            process_memory_usage: IntGauge::new("process_memory_usage_bytes", "Resident memory used by this process").unwrap(),
            process_virtual_memory: IntGauge::new("process_virtual_memory_bytes", "Virtual memory used by this process").unwrap(),
            process_open_fds: IntGauge::new("process_open_fds", "Number of open file descriptors").unwrap(),
            process_threads: IntGauge::new("process_threads", "Thread count of this process").unwrap(),
            process_start_time: IntGauge::new("process_start_time_seconds", "Process start time (UNIX seconds)").unwrap(),
            process_uptime: IntGauge::new("process_uptime_seconds", "Process uptime seconds").unwrap(),

            registry,
        });

        // Register all metrics in the registry
        let reg = &metrics.registry;
        reg.register(Box::new(metrics.source_fetch_requests.clone())).unwrap();
        reg.register(Box::new(metrics.source_fetch_failures.clone())).unwrap();
        reg.register(Box::new(metrics.source_fetch_duration.clone())).unwrap();
        reg.register(Box::new(metrics.parse_failures.clone())).unwrap();
        reg.register(Box::new(metrics.cached_tokens.clone())).unwrap();
        reg.register(Box::new(metrics.token_expiry_unix.clone())).unwrap();
        reg.register(Box::new(metrics.sink_propagations.clone())).unwrap();
        reg.register(Box::new(metrics.sink_failures.clone())).unwrap();
        reg.register(Box::new(metrics.sink_duration.clone())).unwrap();
        reg.register(Box::new(metrics.config_validation_errors.clone())).unwrap();
        reg.register(Box::new(metrics.up.clone())).unwrap();

        reg.register(Box::new(metrics.process_cpu_usage.clone())).unwrap();
        reg.register(Box::new(metrics.process_memory_usage.clone())).unwrap();
        reg.register(Box::new(metrics.process_virtual_memory.clone())).unwrap();
        reg.register(Box::new(metrics.process_open_fds.clone())).unwrap();
        reg.register(Box::new(metrics.process_threads.clone())).unwrap();
        reg.register(Box::new(metrics.process_start_time.clone())).unwrap();
        reg.register(Box::new(metrics.process_uptime.clone())).unwrap();

        metrics
    }
}

