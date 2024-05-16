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

use crate::node::connection::Connection;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;

mod connection;
mod connection_reader;
mod connection_writer;
mod noise_handler;

type ConnectionMap = HashMap<SocketAddr, mpsc::Sender<Bytes>>;

#[derive(Debug)]
pub struct Node {
    pub seeds: Vec<String>,
    pub bind_address: String,
    pub static_key_pem: String,
    pub connections_map: ConnectionMap,
}

impl Node {
    /// Use builder pattern
    pub fn new() -> Self {
        Node {
            seeds: vec!["localhost:6680".to_string()],
            bind_address: "localhost".to_string(),
            static_key_pem: String::new(),
            connections_map: ConnectionMap::new(),
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
            let (requests_sender, requests_receiver) = mpsc::channel::<Bytes>(32);
            tokio::spawn(async move {
                let connection = Connection::new(stream, requests_receiver);
                connection.start(false, key).await;
            });
            self.connections_map.insert(socket_addr, requests_sender);
            self.send(socket_addr, Bytes::from("Hello world")).await;
        }
    }

    /// Connect to all peers and start reader writer tasks
    pub async fn connect_to_seeds(&mut self) {
        log::debug!("Connecting to seeds...");
        for seed in self.seeds.iter() {
            log::debug!("Connecting to seed {}", seed);
            let key = self.static_key_pem.clone();
            if let Ok(stream) = TcpStream::connect(seed).await {
                let peer_addr = stream.peer_addr().unwrap();
                let (requests_sender, requests_receiver) = mpsc::channel::<Bytes>(32);
                tokio::spawn(async move {
                    let connection = Connection::new(stream, requests_receiver);
                    connection.start(true, key).await;
                });
                self.connections_map.insert(peer_addr, requests_sender);
            } else {
                log::debug!("Failed to connect to seed {}", seed);
            }
        }
    }

    /// Send a message to a given node
    pub async fn send(&self, addr: SocketAddr, message: Bytes) {
        let sender = self.connections_map.get(&addr).unwrap();
        let _ = sender.send(message).await;
    }
}
