use clap::{Parser, Subcommand};
use eyre::Result;
use liquidation_bot::monitoring::{LiquidationMonitor, LiquidationMonitorConfig};
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "liquidation-monitor")]
#[command(about = "Monitor and analyze Aave liquidation events", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to configuration file (optional, uses env vars if not provided)
    #[arg(short, long)]
    config: Option<String>,

    /// Override RPC URL
    #[arg(long)]
    rpc_url: Option<String>,

    /// Override WebSocket URL
    #[arg(long)]
    ws_url: Option<String>,

    /// Override pool address
    #[arg(long)]
    pool_address: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Log liquidations to file
    #[arg(long)]
    log_file: Option<String>,

    /// Statistics reporting interval in minutes
    #[arg(long, default_value = "30")]
    stats_interval: u64,
}

#[derive(Subcommand)]
enum Commands {
    /// Start monitoring liquidation events in real-time
    Monitor {
        /// Maximum number of events to keep in memory
        #[arg(long, default_value = "1000")]
        max_events: usize,
    },

    /// Analyze historical liquidation events
    Historical {
        /// Starting block number
        #[arg(long)]
        from_block: u64,

        /// Ending block number (current if not specified)
        #[arg(long)]
        to_block: Option<u64>,
    },

    /// Generate a sample configuration file
    GenerateConfig {
        /// Output path for the configuration file
        #[arg(default_value = "liquidation-monitor-config.json")]
        output: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .init();

    // Print banner
    print_banner();

    // Handle commands
    match cli.command {
        Some(Commands::GenerateConfig { output }) => {
            generate_config(&output)?;
        }
        Some(Commands::Historical {
            from_block,
            to_block,
        }) => {
            run_historical_analysis(&cli, from_block, to_block).await?;
        }
        Some(Commands::Monitor { max_events }) => {
            run_monitor(&cli, max_events).await?;
        }
        None => {
            // Default to monitoring if no command specified
            run_monitor(&cli, 1000).await?;
        }
    }

    Ok(())
}

fn print_banner() {
    println!(
        r#"
    ╔══════════════════════════════════════════╗
    ║     AAVE LIQUIDATION MONITOR v1.0.0     ║
    ║         Real-time Event Tracking         ║
    ╚══════════════════════════════════════════╝
    "#
    );
}

fn generate_config(output: &str) -> Result<()> {
    info!("Generating sample configuration file: {}", output);

    let config = LiquidationMonitorConfig::default();
    config.save_to_file(output)?;

    info!("✅ Configuration file generated successfully!");
    info!("Edit {} to customize your settings", output);

    println!("\nSample configuration:");
    println!("{}", config.summary());

    Ok(())
}

async fn run_monitor(cli: &Cli, max_events: usize) -> Result<()> {
    // Load configuration
    let mut config = if let Some(ref config_path) = cli.config {
        info!("Loading configuration from: {}", config_path);
        LiquidationMonitorConfig::from_file(config_path)?
    } else {
        info!("Loading configuration from environment variables");
        LiquidationMonitorConfig::from_env()?
    };

    // Apply command line overrides
    if let Some(ref rpc_url) = cli.rpc_url {
        config.rpc_url = rpc_url.clone();
    }
    if let Some(ref ws_url) = cli.ws_url {
        config.ws_url = Some(ws_url.clone());
    }
    if let Some(ref pool_address) = cli.pool_address {
        config.pool_address = pool_address.parse()?;
    }
    if let Some(ref log_file) = cli.log_file {
        config.log_to_file = true;
        config.log_file_path = Some(log_file.clone());
    }
    config.stats_interval_minutes = cli.stats_interval;
    config.verbose = cli.verbose;
    config.max_events_stored = max_events;

    // Validate configuration
    config.validate()?;

    // Print configuration summary
    info!("Starting liquidation monitor with configuration:");
    println!("{}", config.summary());

    // Create monitor
    let rpc_url = config.ws_url.as_ref().unwrap_or(&config.rpc_url);
    let monitor = Arc::new(
        LiquidationMonitor::new(
            rpc_url,
            config.pool_address,
            config.max_events_stored,
            config.log_to_file,
            config.log_file_path.clone(),
        )
        .await?,
    );

    // Start statistics reporting
    if config.stats_interval_minutes > 0 {
        info!(
            "📊 Statistics will be reported every {} minutes",
            config.stats_interval_minutes
        );
        monitor
            .clone()
            .start_stats_reporting(config.stats_interval_minutes)
            .await;
    }

    // Start monitoring
    info!("🚀 Starting liquidation event monitoring...");
    info!("Press Ctrl+C to stop\n");

    // Set up graceful shutdown
    let monitor_clone = monitor.clone();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("\n📊 Final Statistics:");
                monitor_clone.print_stats_summary().await;
                info!("👋 Shutting down gracefully...");
                std::process::exit(0);
            }
            Err(err) => {
                error!("Unable to listen for shutdown signal: {}", err);
            }
        }
    });

    // Start monitoring (this will run indefinitely)
    monitor.start_monitoring().await?;

    Ok(())
}

async fn run_historical_analysis(cli: &Cli, from_block: u64, to_block: Option<u64>) -> Result<()> {
    // Load configuration
    let mut config = if let Some(ref config_path) = cli.config {
        info!("Loading configuration from: {}", config_path);
        LiquidationMonitorConfig::from_file(config_path)?
    } else {
        info!("Loading configuration from environment variables");
        LiquidationMonitorConfig::from_env()?
    };

    // Apply command line overrides
    if let Some(ref rpc_url) = cli.rpc_url {
        config.rpc_url = rpc_url.clone();
    }
    if let Some(ref pool_address) = cli.pool_address {
        config.pool_address = pool_address.parse()?;
    }

    config.historical_from_block = Some(from_block);
    config.historical_to_block = to_block;

    info!(
        "🔍 Analyzing historical liquidations from block {} to {}",
        from_block,
        to_block.map_or("latest".to_string(), |b| b.to_string())
    );

    // Create monitor for historical analysis
    let monitor = LiquidationMonitor::new(
        &config.rpc_url,
        config.pool_address,
        10000, // Store more events for historical analysis
        config.log_to_file,
        config.log_file_path.clone(),
    )
    .await?;

    // Fetch and analyze historical events
    analyze_historical_events(Arc::new(monitor), from_block, to_block).await?;

    Ok(())
}

async fn analyze_historical_events(
    monitor: Arc<LiquidationMonitor>,
    from_block: u64,
    to_block: Option<u64>,
) -> Result<()> {
    use alloy_primitives::Address;
    use alloy_provider::{Provider, ProviderBuilder};
    use alloy_rpc_types::{BlockNumberOrTag, Filter};
    use alloy_sol_types::SolEvent;
    use liquidation_bot::models::LiquidationCall;

    info!("📚 Fetching historical liquidation events...");

    // Get provider from monitor's config
    let provider = ProviderBuilder::new().on_http("http://localhost:8545".parse()?);

    let current_block = if let Some(to) = to_block {
        to
    } else {
        provider.get_block_number().await?
    };

    // Create filter for historical events
    let event_signature = LiquidationCall::SIGNATURE_HASH;
    let pool_address: Address = "0xA238Dd80C259a72e81d7e4664a9801593F98d1c5".parse()?;
    let filter = Filter::new()
        .address(pool_address)
        .event_signature(event_signature)
        .from_block(BlockNumberOrTag::Number(from_block))
        .to_block(BlockNumberOrTag::Number(current_block));

    // Fetch logs
    let logs = provider.get_logs(&filter).await?;

    info!(
        "Found {} liquidation events in the specified range",
        logs.len()
    );

    // Process each log
    for (i, log) in logs.iter().enumerate() {
        // Convert alloy_rpc_types::Log to alloy_primitives::Log
        let primitive_log = alloy_primitives::Log {
            address: log.address(),
            data: alloy_primitives::LogData::new_unchecked(
                log.topics().to_vec(),
                log.data().data.clone(),
            ),
        };

        if let Ok(event) = LiquidationCall::decode_log(&primitive_log, true) {
            info!(
                "Event {}/{}: User {:?} liquidated by {:?}",
                i + 1,
                logs.len(),
                event.user,
                event.liquidator
            );
        }
    }

    // Print final statistics
    monitor.print_stats_summary().await;

    Ok(())
}
