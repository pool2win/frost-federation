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

use tokio::sync::mpsc;

use crate::node::Node;

/// Trait for commands interface to Node
pub trait Commands {
    async fn start_command_loop(&self, command_rx: mpsc::Receiver<String>);
}

impl Commands for Node {
    /// Command event loop receives commands from RPC
    async fn start_command_loop(&self, mut command_rx: mpsc::Receiver<String>) {
        while let Some(msg) = command_rx.recv().await {
            println!("Received {:?}", msg);
            match msg.as_str() {
                "shutdown" => {
                    println!("Shutting down....");
                    return;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod command_tests {
    use super::Node;
    #[mockall_double::double]
    use crate::node::echo_broadcast::EchoBroadcastHandle;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn it_should_run_node_with_command_rx() {
        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(EchoBroadcastHandle::default);

        let (command_tx, command_rx) = mpsc::channel(10);
        let mut node = Node::new()
            .await
            .seeds(vec![])
            .bind_address("localhost:6880".to_string())
            .static_key_pem("a key".to_string())
            .delivery_timeout(1000);

        let node_task = node.start(command_rx);
        // Node stays up for a non-shutdown command
        let _ = command_tx.send("test command".into()).await;
        // Node shuts down on shutdown command
        let _ = command_tx.send("shutdown".into()).await;
        let _ = tokio::join!(node_task);
    }
}
