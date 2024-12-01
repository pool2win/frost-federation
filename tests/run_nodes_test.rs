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
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[test]
    fn test_start_two_nodes_and_let_them_connect_without_an_error() {
        let _ = env_logger::builder().is_test(true).try_init();

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut node = Node::new("localhost:6880".to_string(), vec![])
                    .await
                    .static_key_pem(KEY.into())
                    .delivery_timeout(100);

                let (ready_tx, mut ready_rx) = oneshot::channel();
                let (_executor, command_rx) = CommandExecutor::new();
                let node_task = async {
                    node.start(command_rx, ready_tx).await;
                };

                let mut node_b =
                    Node::new("localhost:6881".to_string(), vec!["localhost:6880".into()])
                        .await
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

    #[test]
    fn test_dkg_completes() {
        let _ = env_logger::builder().is_test(true).try_init();

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                // Start node A
                let mut node_a = Node::new("localhost:6890".to_string(), vec![])
                    .await
                    .static_key_pem(KEY.into())
                    .delivery_timeout(100);

                let (ready_tx_a, ready_rx_a) = oneshot::channel();
                let (executor_a, command_rx_a) = CommandExecutor::new();
                let node_a_task = async {
                    node_a.start(command_rx_a, ready_tx_a).await;
                };

                // Start node B
                let mut node_b =
                    Node::new("localhost:6891".to_string(), vec!["localhost:6890".into()])
                        .await
                        .static_key_pem(KEY.into())
                        .delivery_timeout(100);

                let (ready_tx_b, ready_rx_b) = oneshot::channel();
                let (executor_b, command_rx_b) = CommandExecutor::new();
                let node_b_task = async {
                    let _ = ready_rx_a.await;
                    node_b.start(command_rx_b, ready_tx_b).await;
                };

                // Start node C
                let mut node_c = Node::new(
                    "localhost:6892".to_string(),
                    vec!["localhost:6890".into(), "localhost:6891".into()],
                )
                .await
                .static_key_pem(KEY.into())
                .delivery_timeout(100);

                let (ready_tx_c, _ready_rx_c) = oneshot::channel();
                let (executor_c, command_rx_c) = CommandExecutor::new();
                let node_c_task = async {
                    let _ = ready_rx_b.await;
                    node_c.start(command_rx_c, ready_tx_c).await;
                };

                // Spawn task to run DKG once all nodes are connected
                tokio::spawn(async move {
                    // Wait for all nodes to connect to each other
                    loop {
                        let members_a = executor_a.get_members().await.unwrap();
                        let members_b = executor_b.get_members().await.unwrap();
                        let members_c = executor_c.get_members().await.unwrap();

                        log::info!("members_a: {:?}", members_a);
                        log::info!("members_b: {:?}", members_b);
                        log::info!("members_c: {:?}", members_c);
                        if members_a.len() == 2 && members_b.len() == 2 && members_c.len() == 2 {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }

                    // Run DKG on all nodes
                    let _ = executor_a.run_dkg().await;
                    let _ = executor_b.run_dkg().await;
                    let _ = executor_c.run_dkg().await;

                    // Wait for DKG to complete and get public keys
                    loop {
                        let key_a = executor_a.get_dkg_public_key().await.unwrap();
                        let key_b = executor_b.get_dkg_public_key().await.unwrap();
                        let key_c = executor_c.get_dkg_public_key().await.unwrap();

                        if let (Some(key_a), Some(key_b), Some(key_c)) = (key_a, key_b, key_c) {
                            // Verify all public keys match
                            assert_eq!(key_a, key_b);
                            assert_eq!(key_b, key_c);

                            // Shutdown nodes
                            let _ = executor_a.shutdown().await;
                            let _ = executor_b.shutdown().await;
                            let _ = executor_c.shutdown().await;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                });

                tokio::select! {
                    _ = node_a_task => {}
                    _ = node_b_task => {}
                    _ = node_c_task => {}
                }
            });
    }
}
