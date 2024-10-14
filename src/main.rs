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
use tokio::sync::mpsc;

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

    let (_commands, command_rx) = CommandExecutor::new();
    let mut node = node::Node::new()
        .await
        .seeds(config.peer.seeds)
        .bind_address(bind_address)
        .static_key_pem(config.noise.key)
        .delivery_timeout(config.peer.delivery_timeout);

    let (ready_tx, _ready_rx) = mpsc::channel(1);
    node.start(command_rx, ready_tx).await;
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
