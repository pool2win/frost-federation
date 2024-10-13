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

    use frost_federation::node;

    #[test]
    fn test_start_two_nodes_and_let_them_connect_without_an_error() {
        use frost_federation::node::commands::CommandExecutor;

        tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let mut node_b = node::Node::new()
                .await
                .seeds(vec!["localhost:6880".into()])
                .bind_address("localhost:6881".into())
                .static_key_pem("MFECAQEwBQYDK2VwBCIEIPCN4nC8Zn9jEKBc4jiUCPcHNdqQ8WgpyUv09eKJHSxfgSEAtJysJ2e6m4ze8Kz1zYjBByVR4EO/7iGRkTtd7cYOGi0=".into())
                .delivery_timeout(100);


            let (executor_b, command_rx_b) = CommandExecutor::new();
            let node_b_task = node_b.start(command_rx_b);

            let mut node = node::Node::new()
                .await
                .seeds(vec![])
                .bind_address("localhost:6880".into())
                .static_key_pem("MFECAQEwBQYDK2VwBCIEIJ7pILqR7yBPsVuysfGyofjOEm19skmtqcJYWiVwjKH1gSEA68zeZuy7PMMQC9ECPmWqDl5AOFj5bi243F823ZVWtXY=".into())
                .delivery_timeout(100);

            let (_executor, command_rx) = CommandExecutor::new();
            let node_task = node.start(command_rx);
            tokio::spawn(async move {
                while let Ok(members) = executor_b.get_members().await {
                    if members.len() == 1 {
                        assert_eq!(members.len(), 1);
                        let _ = executor_b.shutdown().await;
                    }
                }});
            tokio::select! {
                _ = node_task => {}
                _ = node_b_task => {}
            }
        });
    }
}
