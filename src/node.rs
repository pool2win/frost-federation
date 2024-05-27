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

use crate::node::connection::ConnectionHandle;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::bytes::Bytes;

mod connection;
mod protocol;

#[derive(Debug)]
pub struct Node {
    pub seeds: Vec<String>,
    pub bind_address: String,
    pub static_key_pem: String,
}

impl Node {
    /// Use builder pattern
    pub fn new() -> Self {
        Node {
            seeds: vec!["localhost:6680".to_string()],
            bind_address: "localhost".to_string(),
            static_key_pem: String::new(),
        }
    }

    pub fn seeds(self, seeds: Vec<String>) -> Self {
        let mut node = self;
        node.seeds = seeds;
        node
    }

    pub fn bind_address(self, address: String) -> Self {
        let mut node = self;
        node.bind_address = address;
        node
    }

    pub fn static_key_pem(self, key: String) -> Self {
        let mut node = self;
        node.static_key_pem = key;
        node
    }

    /// Start node by listening, accepting and connecting to peers
    pub async fn start(&mut self) {
        log::debug!("Starting...");
        self.connect_to_seeds().await;
        let listener = self.listen().await;
        self.start_accept(listener).await;
    }

    /// Start listening
    pub async fn listen(&mut self) -> TcpListener {
        log::debug!("Start listen...");
        TcpListener::bind(self.bind_address.to_string())
            .await
            .unwrap()
    }

    /// Start accepting connections
    pub async fn start_accept(&mut self, listener: TcpListener) {
        log::debug!("Start accepting...");
        loop {
            log::debug!("Waiting on accept...");
            let (stream, socket_addr) = listener.accept().await.unwrap();
            log::info!("Accept connection from {}", socket_addr);
            let key = self.static_key_pem.clone();
            let connection_handle = ConnectionHandle::new(stream);
            let send_connection_handle = connection_handle.clone();

            self.start_connection(connection_handle, send_connection_handle)
                .await;
        }
    }

    /// Connect to all peers and start reader writer tasks
    pub async fn connect_to_seeds(&mut self) {
        log::debug!("Connecting to seeds...");
        let seeds = self.seeds.clone();
        for seed in seeds.iter() {
            log::debug!("Connecting to seed {}", seed);
            let key = self.static_key_pem.clone();
            if let Ok(stream) = TcpStream::connect(seed).await {
                let peer_addr = stream.peer_addr().unwrap();
                let connection_handle = ConnectionHandle::new(stream);
                let send_connection_handle = connection_handle.clone();

                self.start_connection(connection_handle, send_connection_handle)
                    .await;
            } else {
                log::debug!("Failed to connect to seed {}", seed);
            }
        }
    }

    pub async fn start_connection(
        &mut self,
        connection_handle: ConnectionHandle,
        send_connection_handle: ConnectionHandle,
    ) {
        // send demo hello world
        let _ = send_connection_handle
            .send(Bytes::from("Hello world"))
            .await;

        // start receiving messages
        let mut receiver = connection_handle.start_subscription().await;
        tokio::spawn(async move {
            while let Some(result) = receiver.recv().await {
                log::info!("Received {:?}", result);
            }
            log::debug!("Closing accepted connection");
        });
    }
}
