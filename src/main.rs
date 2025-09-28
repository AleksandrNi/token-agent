use token_agent::config::loader::load_config;
use token_agent::sources::{build_source, SourceKind};
use token_agent::dag::scheduler::DagScheduler;
use token_agent::sinks::{file_sink::FileSink, metrics_sink::MetricsSink};
use token_agent::endpoints::udp::UdsEndpoint;
use token_agent::endpoints::metrics::MetricsEndpoint;
use tokio::sync::watch;
use std::collections::HashMap;
use tokio::task;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Load YAML config
    let cfg = load_config("config.yaml")?;

    // 2. Build sources
    let mut sources: HashMap<String, SourceKind> = HashMap::new();
    for (name, src_cfg) in &cfg.sources {
        sources.insert(name.clone(), build_source(name, src_cfg));
    }

    // 3. Run DAG scheduler to fetch all tokens
    let scheduler = DagScheduler::new(sources.clone());
    scheduler.fetch_all().await?;

    // 4. Initialize file sinks and metrics sinks
    let mut file_sinks = Vec::new();
    let mut metrics_sinks = Vec::new();
    let (tx, rx) = watch::channel(());

    for (name, sink_cfg) in &cfg.sinks {
        match sink_cfg.sink_type.as_str() {
            "file" => {
                let source = sink_cfg.inputs.as_ref()
                    .and_then(|v| v.get(0))
                    .expect("File sink must have one input")
                    .clone();
                let sink = FileSink::new(name.clone(), sink_cfg, source)?;
                if sink.watch {
                    let mut rx_clone = rx.clone();
                    let sink_clone = sink.clone();
                    task::spawn(async move { sink_clone.watch_loop(rx_clone).await.unwrap(); });
                }
                sink.write().await?;
                file_sinks.push(sink);
            }
            "metrics" => {
                let source = sink_cfg.inputs.as_ref()
                    .and_then(|v| v.get(0))
                    .expect("Metrics sink must have one input")
                    .clone();
                let sink = MetricsSink::new(&source)?;
                sink.update().await?;
                metrics_sinks.push(sink);
            }
            t => panic!("Unsupported sink type '{}'", t),
        }
    }

    // 5. Start UDS endpoints
    for (name, ep_cfg) in &cfg.endpoints {
        match ep_cfg.endpoint_type.as_str() {
            "uds" => {
                let source = ep_cfg.inputs.as_ref()
                    .and_then(|v| v.get(0))
                    .expect("UDS endpoint must have one input")
                    .clone();
                let uds = UdsEndpoint::new(ep_cfg.path.as_ref().unwrap(), &source);
                task::spawn(async move { uds.run().await.unwrap(); });
            }
            "http" => {
                if ep_cfg.path.as_ref().unwrap() == "/metrics" {
                    let metrics = MetricsEndpoint::new("127.0.0.1:9100");
                    task::spawn(async move { metrics.run().await.unwrap(); });
                }
            }
            t => panic!("Unsupported endpoint type '{}'", t),
        }
    }

    println!("Token service running...");
    futures::future::pending::<()>().await;
    Ok(())
}
