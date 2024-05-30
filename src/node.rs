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

use std::result;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;

use self::connection::ConnectionHandle;
use self::noise_handler::NoiseHandler;
use self::protocol::HandshakeMessage;
use self::protocol::Message;
mod connection;
mod noise_handler;
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
            let (connection_handle, subscription_receiver) = ConnectionHandle::start(stream);
            self.start_connection(connection_handle, true, subscription_receiver)
                .await;
        }
    }

    /// Connect to all peers and start reader writer tasks
    pub async fn connect_to_seeds(&mut self) {
        log::debug!("Connecting to seeds...");
        let seeds = self.seeds.clone();
        for seed in seeds.iter() {
            log::debug!("Connecting to seed {}", seed);
            if let Ok(stream) = TcpStream::connect(seed).await {
                let peer_addr = stream.peer_addr().unwrap();
                log::info!("Connected to {}", peer_addr);
                let (connection_handle, subscription_receiver) = ConnectionHandle::start(stream);
                self.start_connection(connection_handle, false, subscription_receiver)
                    .await;
            } else {
                log::debug!("Failed to connect to seed {}", seed);
            }
        }
    }

    pub async fn start_connection(
        &mut self,
        connection_handle: ConnectionHandle,
        init: bool,
        subscription_receiver: mpsc::Receiver<Bytes>,
    ) {
        let key = self.static_key_pem.clone();
        let mut noise = NoiseHandler::new(init, key);

        let mut subscription_receiver = noise
            .run_handshake(connection_handle.clone(), subscription_receiver)
            .await;
        noise.start_transport();
        log::debug!("Noise transport started");

        let cloned = connection_handle.clone();
        tokio::spawn(async move {
            while let Some(result) = subscription_receiver.recv().await {
                let result = noise.read_transport_message(result);
                let received_message = Message::from_bytes(&result).unwrap();
                log::debug!("Received {:?}", received_message);
                if let Some(response) = received_message.response_for_received().unwrap() {
                    log::debug!("Sending Response {:?}", response);
                    if let Some(response_bytes) = response.as_bytes() {
                        let response = noise.build_transport_message(&response_bytes);
                        let _ = cloned.send(response).await;
                    }
                }
            }
            log::debug!("Closing accepted connection");
        });
        // protocol::start_protocol::<HandshakeMessage>(connection_handle, init).await;
    }
}
