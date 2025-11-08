use clap::arg;
use clap::command;
use clap::Parser;
use reqwest::Client;
use token_agent::observability::service_resources_metrics::collect_process_metrics;
use token_agent::server;
use token_agent::sinks::manager::SinkManager;
use token_agent::sources::builder_in_order::SourceDag;
use token_agent::utils::channel;
use token_agent::utils::config_loader;
use token_agent::utils::logging;
use anyhow::Result;
use token_agent::utils::logging::LogLevel;
use tracing::info;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, env = "CONFIG", default_value = "token-agent.yaml")]
    config: String,
    #[arg(long, env = "LOG_LEVEL" , value_enum)]
    log_level: Option<LogLevel>,
    // #[arg(long)]
    // watch_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // -------------------------------
    // 1. Make preparations
    // 
    // read env
    // create channel
    // -------------------------------
    
    let args = Args::parse();
    let sink_sender = channel::run();
    
    // -------------------------------
    // 2. Load YAML config
    // -------------------------------

    let service_config = config_loader::run(&args.config).await?;
    logging::run(&service_config, args.log_level.to_owned()).await?;

    // -------------------------------
    // 3. Prepare sources dependency graph
    // -------------------------------

    let dag = SourceDag::build(&service_config.sources)?;

    // -------------------------------
    // 4. Create request client
    // -------------------------------

    let client = Client::new();


    // -------------------------------
    // SOURCES
    // -------------------------------


    // -------------------------------
    // 5. Fetch tokens for each source according to dependecy graph order
    // -------------------------------

    // -------------------------------
    // 5.1. Prepare fetch tokens worker
    // -------------------------------

    let safety_margin_seconds = service_config.settings.safety_margin_seconds;
    let retry = &service_config.settings.retry;
    let receiver = dag.loop_refrech_tokens(&client, &retry, safety_margin_seconds, sink_sender.clone());

    // -------------------------------
    // 5.2. Prepare cleanup expired tokens worker
    // -------------------------------

    let cleaner = dag.loop_check_token_exp(&service_config.sources, &safety_margin_seconds, sink_sender.clone());


    // -------------------------------
    // SINKS
    // -------------------------------


    // -------------------------------
    // 6. Start file, udp (actve) sinks
    // -------------------------------

    let sink_manager = SinkManager::new(service_config.sinks.to_owned());
    let active_sinks = sink_manager.start_active_sinks(sink_sender.clone());

    // -------------------------------
    // 7. Start http server with http (pasive) sink
    // -------------------------------    

    let http_server = server::server::start(&service_config.settings, &service_config.sinks);


    // -------------------------------
    // METRICS
    // -------------------------------


    // -------------------------------
    // 8. Start scraping system resources consupption metrics
    // -------------------------------    

    let service_metrics = collect_process_metrics(service_config.settings.metrics.is_enabled.to_owned());
    info!("Service starting...");
    tokio::try_join!(receiver, cleaner, active_sinks, http_server, service_metrics)?;

    Ok(())
}