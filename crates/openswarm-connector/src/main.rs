//! CLI binary entry point for the OpenSwarm Connector sidecar.
//!
//! Usage:
//!   openswarm-connector [OPTIONS]
//!
//! Options:
//!   -c, --config <FILE>    Path to configuration TOML file
//!   -l, --listen <ADDR>    P2P listen address (overrides config)
//!   -r, --rpc <ADDR>       RPC bind address (overrides config)
//!   -b, --bootstrap <ADDR> Bootstrap peer multiaddress (repeatable)
//!   -v, --verbose          Increase logging verbosity
//!   --agent-name <NAME>    Set the agent name

use std::path::PathBuf;

use clap::Parser;

use openswarm_connector::config::ConnectorConfig;
use openswarm_connector::connector::OpenSwarmConnector;
use openswarm_connector::rpc_server::RpcServer;

/// OpenSwarm Connector - Sidecar process connecting AI agents to the swarm.
#[derive(Parser, Debug)]
#[command(name = "openswarm-connector")]
#[command(about = "OpenSwarm Connector sidecar for AI agent swarm participation")]
#[command(version)]
struct Cli {
    /// Path to configuration TOML file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// P2P listen address (e.g., /ip4/0.0.0.0/tcp/9000).
    #[arg(short, long, value_name = "MULTIADDR")]
    listen: Option<String>,

    /// JSON-RPC server bind address (e.g., 127.0.0.1:9370).
    #[arg(short, long, value_name = "ADDR")]
    rpc: Option<String>,

    /// Bootstrap peer multiaddress (can be specified multiple times).
    #[arg(short, long, value_name = "MULTIADDR")]
    bootstrap: Vec<String>,

    /// Increase logging verbosity (can be repeated: -v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Set the agent name.
    #[arg(long, value_name = "NAME")]
    agent_name: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load configuration.
    let mut config = ConnectorConfig::load(cli.config.as_deref())?;

    // Apply CLI overrides.
    if let Some(listen) = cli.listen {
        config.network.listen_addr = listen;
    }
    if let Some(rpc) = cli.rpc {
        config.rpc.bind_addr = rpc;
    }
    if !cli.bootstrap.is_empty() {
        config.network.bootstrap_peers = cli.bootstrap;
    }
    if let Some(name) = cli.agent_name {
        config.agent.name = name;
    }

    // Adjust log level based on verbosity.
    let log_level = match cli.verbose {
        0 => &config.logging.level,
        1 => "debug",
        2 => "trace",
        _ => "trace",
    };

    // Initialize logging.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    tracing::info!(
        agent = %config.agent.name,
        listen = %config.network.listen_addr,
        rpc = %config.rpc.bind_addr,
        "Starting OpenSwarm Connector"
    );

    // Create the connector.
    let connector = OpenSwarmConnector::new(config.clone())?;

    // Get handles for the RPC server.
    let state = connector.shared_state();
    let network_handle = connector.network_handle();

    // Start the RPC server in a background task.
    let rpc_server = RpcServer::new(
        config.rpc.bind_addr.clone(),
        state,
        network_handle,
        config.rpc.max_connections,
    );

    tokio::spawn(async move {
        if let Err(e) = rpc_server.run().await {
            tracing::error!(error = %e, "RPC server error");
        }
    });

    // Run the connector (this blocks until shutdown).
    connector.run().await?;

    Ok(())
}
