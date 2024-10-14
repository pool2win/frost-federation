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

mod node_integration_tests {
    // Use the same noise public key for all nodes in these tests
    const KEY: &str = "
-----BEGIN PRIVATE KEY-----
MFECAQEwBQYDK2VwBCIEIJ7pILqR7yBPsVuysfGyofjOEm19skmtqcJYWiVwjKH1
gSEA68zeZuy7PMMQC9ECPmWqDl5AOFj5bi243F823ZVWtXY=
-----END PRIVATE KEY-----
";

    use frost_federation::node::commands::CommandExecutor;
    use frost_federation::node::Node;
    use tokio::sync::oneshot;

    #[test]
    fn test_start_two_nodes_and_let_them_connect_without_an_error() {
        let _ = env_logger::builder().is_test(true).try_init();

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut node = Node::new()
                    .await
                    .seeds(vec![])
                    .bind_address("localhost:6880".into())
                    .static_key_pem(KEY.into())
                    .delivery_timeout(100);

                let (ready_tx, mut ready_rx) = oneshot::channel();
                let (_executor, command_rx) = CommandExecutor::new();
                let node_task = async {
                    node.start(command_rx, ready_tx).await;
                };

                let mut node_b = Node::new()
                    .await
                    .seeds(vec!["localhost:6880".into()])
                    .bind_address("localhost:6881".into())
                    .static_key_pem(KEY.into())
                    .delivery_timeout(100);

                let (ready_tx_b, mut _ready_rx_b) = oneshot::channel();
                let (executor_b, command_rx_b) = CommandExecutor::new();
                let node_b_task = async {
                    let _ = ready_rx.await;
                    node_b.start(command_rx_b, ready_tx_b).await;
                };

                tokio::spawn(async move {
                    while let Ok(members) = executor_b.get_members().await {
                        if members.len() == 1 {
                            assert_eq!(members.len(), 1);
                            let _ = executor_b.shutdown().await;
                        }
                    }
                });

                tokio::select! {
                    _ = node_task => {}
                    _ = node_b_task => {}
                }
            });
    }
}
