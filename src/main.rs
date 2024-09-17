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
use std::error::Error;
use tokio::sync::broadcast;
use tokio_util::bytes::Bytes;

mod cli;
mod config;
mod node;

#[actix::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let config = config::load_config_from_file(args.config_file).unwrap();
    let bind_address = config::get_bind_address(config.network);

    setup_logging()?;
    setup_tracing()?;

    // let manager = Arc::new(ConnectionManager::new(config.peer.max_peer_count));

    let (send_to_all_tx, _) = broadcast::channel::<Bytes>(config.peer.max_pending_send_to_all);
    let _connect_broadcast_sender = send_to_all_tx.clone();
    let _listen_broadcast_sender = send_to_all_tx.clone();

    let mut node = node::Node::new()
        .await
        .seeds(config.peer.seeds)
        .bind_address(bind_address)
        .static_key_pem(config.noise.key)
        .delivery_timeout(config.peer.delivery_timeout);

    node.start().await;
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
