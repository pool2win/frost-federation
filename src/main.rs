// Copyright 2024 Kulpreet Singh

// This file is part of Frost-Federation

// Frost-Federation is free software: you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.

// Frost-Federation is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Frost-Federation. If not, see
// <https://www.gnu.org/licenses/>.

use clap::Parser;
use frost_federation::node::commands::CommandExecutor;
use std::error::Error;
use tokio::sync::oneshot;

use frost_federation::cli;
use frost_federation::config;
use frost_federation::node;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let config = config::load_config_from_file(args.config_file).unwrap();
    let bind_address = config::get_bind_address(config.network);

    setup_logging()?;
    setup_tracing()?;

    let (command_executor, command_rx) = CommandExecutor::new();
    let mut node = node::Node::new(bind_address, config.peer.seeds)
        .await
        .static_key_pem(config.noise.key)
        .delivery_timeout(config.peer.delivery_timeout);

    // Spawn a task to run DKG after 5 seconds
    // Only run DKG if RUN_DKG env var is set to 1
    if std::env::var("RUN_DKG").unwrap_or_default() == "1" {
        let executor = command_executor.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if let Err(e) = executor.run_dkg().await {
                log::error!("Failed to run DKG: {}", e);
            }
        });
    }

    // Start node
    let (ready_tx, _ready_rx) = oneshot::channel();
    node.start(command_rx, ready_tx).await;
    log::info!("Stopping node");
    Ok(())
}

fn setup_tracing() -> Result<(), Box<dyn Error>> {
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}

fn setup_logging() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    Ok(())
}
