use frost_federation::config;
use frost_federation::node;

#[test]
fn test_start_a_single_node_should_complete_without_error() {
    use tokio::time::{timeout, Duration};
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let config = config::load_config_from_file("config.run".to_string()).unwrap();
            let bind_address = config::get_bind_address(config.network);

            let mut node = node::Node::new()
                .await
                .seeds(config.peer.seeds)
                .bind_address(bind_address)
                .static_key_pem(config.noise.key)
                .delivery_timeout(config.peer.delivery_timeout);

            let _ = timeout(Duration::from_millis(10), node.start()).await;
        });
}
