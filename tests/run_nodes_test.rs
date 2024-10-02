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
