use clap::Parser;
use std::error::Error;
use tokio::sync::broadcast;
use tokio_util::bytes::Bytes;

mod cli;
mod config;
mod node;

// use connection::connection_manager::ConnectionManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = cli::Cli::parse();

    let config = config::load_config_from_file(args.config_file).unwrap();
    let bind_address = config::get_bind_address(config.network);

    setup_logging()?;
    setup_tracing()?;

    // let manager = Arc::new(ConnectionManager::new(config.peer.max_peer_count));

    let (send_to_all_tx, _) = broadcast::channel::<Bytes>(config.peer.max_pending_send_to_all);
    let connect_broadcast_sender = send_to_all_tx.clone();
    let listen_broadcast_sender = send_to_all_tx.clone();

    // let (reset_notifier, _) = connection::start_heartbeat(
    //     bind_address.clone(),
    //     config.peer.heartbeat_interval,
    //     send_to_all_tx,
    //     manager.clone(),
    // )
    // .await;

    // let mut node = node::Node::new().
    //     seeds(config.peer.seeds).
    let mut node = node::Node::new()
        .seeds(config.peer.seeds)
        .bind_address(bind_address);

    println!("{:?}", node);
    node.start().await;
    // for seed in config.peer.seeds {
    //     connection::connect(
    //         seed,
    //         manager.clone(),
    //         config.peer.max_pending_messages,
    //         connect_broadcast_sender.subscribe(),
    //         reset_notifier.clone(),
    //     );
    // }

    // connection::start_listen(
    //     bind_address,
    //     manager.clone(),
    //     config.peer.max_pending_messages,
    //     listen_broadcast_sender,
    //     reset_notifier,
    // )
    // .await;
    log::info!("Listen done");
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
