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

use crate::node::Node;
use frost_secp256k1 as frost;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub enum Command {
    Shutdown,
    GetMembers {
        respond_to: oneshot::Sender<Result<Vec<String>, Box<dyn Error + Send>>>,
    },
    GetDKGPublicKey {
        respond_to:
            oneshot::Sender<Result<Option<frost::keys::PublicKeyPackage>, Box<dyn Error + Send>>>,
    },
    RunDKG {
        respond_to: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
}

#[derive(Clone)]
pub struct CommandExecutor {
    tx: mpsc::Sender<Command>,
}

impl CommandExecutor {
    pub fn new() -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel(10);
        (Self { tx }, rx)
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown).await;
    }

    pub async fn get_members(&self) -> Result<Vec<String>, Box<dyn Error + Send>> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Command::GetMembers { respond_to: tx }).await;
        rx.await.unwrap()
    }

    pub async fn get_dkg_public_key(
        &self,
    ) -> Result<Option<frost::keys::PublicKeyPackage>, Box<dyn Error + Send>> {
        let (tx, rx) = oneshot::channel();

        let _ = self
            .tx
            .send(Command::GetDKGPublicKey { respond_to: tx })
            .await;
        rx.await.unwrap()
    }

    pub async fn run_dkg(&self) -> Result<(), Box<dyn Error + Send>> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Command::RunDKG { respond_to: tx }).await;
        rx.await.unwrap()
    }
}

/// Trait for commands interface to Node
pub(crate) trait Commands {
    async fn start_command_loop(&self, command_rx: mpsc::Receiver<Command>);
}

impl Commands for Node {
    /// Command event loop receives commands from RPC
    async fn start_command_loop(&self, mut command_rx: mpsc::Receiver<Command>) {
        log::debug!("Starting command loop....");
        while let Some(msg) = command_rx.recv().await {
            log::debug!("Received command: {:?}", msg);
            match msg {
                Command::Shutdown => {
                    log::info!("Shutting down....");
                    return;
                }
                Command::GetMembers { respond_to } => {
                    let _ = respond_to.send(self.get_members().await);
                }
                Command::GetDKGPublicKey { respond_to } => {
                    let _ = respond_to.send(self.get_dkg_public_key().await);
                }
                Command::RunDKG { respond_to } => {
                    if let Err(e) = self.trigger_dkg_tx.send(()).await {
                        let _ = respond_to.send(Err(Box::new(e)));
                    } else {
                        let _ = respond_to.send(Ok(()));
                    }
                }
            }
        }
        log::debug!("Stopping command loop....");
    }
}

#[cfg(test)]
mod command_tests {
    use super::CommandExecutor;
    use super::Node;
    #[mockall_double::double]
    use crate::node::echo_broadcast::EchoBroadcastHandle;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn it_should_run_node_with_command_rx() {
        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone()
                .returning(|| EchoBroadcastHandle::default());
            mock
        });

        let (exector, command_rx) = CommandExecutor::new();
        let mut node = Node::new()
            .await
            .seeds(vec![])
            .bind_address("localhost:6880".to_string())
            .static_key_pem("a key".to_string())
            .delivery_timeout(1000);

        let (ready_tx, _ready_rx) = oneshot::channel();
        let node_task = node.start(command_rx, ready_tx);
        // Node shuts down on shutdown command
        let _ = exector.shutdown().await;
        node_task.await;
    }

    #[tokio::test]
    async fn it_should_return_none_when_no_dkg_public_key() {
        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone()
                .returning(|| EchoBroadcastHandle::default());
            mock
        });

        let (executor, command_rx) = CommandExecutor::new();
        let mut node = Node::new()
            .await
            .seeds(vec![])
            .bind_address("localhost:6880".to_string())
            .static_key_pem("a key".to_string())
            .delivery_timeout(1000);

        let (ready_tx, _ready_rx) = oneshot::channel();
        let node_task = node.start(command_rx, ready_tx);

        let get_key_and_shutdown = async move {
            // Get public key before DKG has run
            let public_key = executor.get_dkg_public_key().await;
            assert!(public_key.is_ok());
            assert!(public_key.unwrap().is_none());

            let _ = executor.shutdown().await;
        };
        tokio::select! {
            _ = get_key_and_shutdown => {}
            _ = node_task => {}
        }
    }
}
