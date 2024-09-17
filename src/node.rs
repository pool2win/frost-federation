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

use self::{
    membership::{AddMember, Membership, RemoveMember},
    protocol::HandshakeMessage,
    reliable_sender::ReliableNetworkMessage,
};
use crate::node::noise_handler::{NoiseHandler, NoiseIO};
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use actix::Actor;
#[mockall_double::double]
use connection::ConnectionHandle;
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::mpsc::Receiver,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

mod connection;
mod echo_broadcast;
mod membership;
mod noise_handler;
mod protocol;
mod reliable_sender;

pub struct Node {
    pub seeds: Vec<String>,
    pub bind_address: String,
    pub static_key_pem: String,
    pub delivery_timeout: u64,
    pub membership_addr: actix::Addr<Membership>,
}

impl Node {
    /// Use builder pattern
    pub async fn new() -> Self {
        let bind_address = "localhost".to_string();
        Node {
            seeds: vec!["localhost:6680".to_string()],
            bind_address: bind_address.clone(),
            static_key_pem: String::new(),
            delivery_timeout: 500,
            membership_addr: Membership::default().start(),
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

    pub fn delivery_timeout(self, timeout: u64) -> Self {
        let mut node = self;
        node.delivery_timeout = timeout;
        node
    }

    /// Get the node id for self.
    ///
    /// Right now return the bind address. This will be later changed
    /// to using the public key the node uses as an identifier.
    pub fn get_node_id(&self) -> String {
        self.bind_address.clone()
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

    pub fn build_reader_writer(
        &self,
        stream: TcpStream,
    ) -> (
        FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
        FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    ) {
        let (reader, writer) = stream.into_split();
        let framed_reader = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_read(reader);
        let framed_writer = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_write(writer);
        (framed_reader, framed_writer)
    }

    /// Start accepting connections
    pub async fn start_accept(&mut self, listener: TcpListener) {
        log::debug!("Start accepting...");
        let init = true;
        loop {
            log::debug!("Waiting on accept...");
            let (stream, socket_addr) = listener.accept().await.unwrap();
            log::info!("Accept connection from {}", socket_addr);
            let (reader, writer) = self.build_reader_writer(stream);
            let noise = NoiseHandler::new(init, self.static_key_pem.clone());
            let (connection_handle, connection_receiver) =
                ConnectionHandle::start(reader, writer, noise, init).await;
            let reliable_sender_handle = self
                .start_reliable_sender_receiver(
                    connection_handle,
                    connection_receiver,
                    socket_addr.to_string(),
                )
                .await;
            if self
                .membership_addr
                .send(AddMember {
                    member: socket_addr.to_string(),
                    handler: reliable_sender_handle.clone(),
                })
                .await
                .is_err()
            {
                log::debug!("Error adding new connection to membership. Stopping.");
                return;
            }
            let node_id = self.get_node_id();
            // Start the first protocol to start interaction between nodes
            protocol::start_protocol::<HandshakeMessage>(reliable_sender_handle, &node_id).await;
        }
    }

    /// Connect to all peers and start reader writer tasks
    pub async fn connect_to_seeds(&mut self) {
        log::debug!("Connecting to seeds...");
        let seeds = self.seeds.clone();
        let init = false;
        for seed in seeds.iter() {
            log::debug!("Connecting to seed {}", seed);
            if let Ok(stream) = TcpStream::connect(seed).await {
                let peer_addr = stream.peer_addr().unwrap();
                log::info!("Connected to {}", peer_addr);
                let (reader, writer) = self.build_reader_writer(stream);
                let noise = NoiseHandler::new(init, self.static_key_pem.clone());
                let (connection_handle, connection_receiver) =
                    ConnectionHandle::start(reader, writer, noise, init).await;
                let reliable_sender_handle = self
                    .start_reliable_sender_receiver(
                        connection_handle,
                        connection_receiver,
                        peer_addr.to_string(),
                    )
                    .await;
                if self
                    .membership_addr
                    .send(AddMember {
                        member: peer_addr.to_string(),
                        handler: reliable_sender_handle,
                    })
                    .await
                    .is_err()
                {
                    log::debug!("Error adding new connection to membership. Stopping.");
                }
            } else {
                log::debug!("Failed to connect to seed {}", seed);
            }
        }
    }

    pub async fn start_reliable_sender_receiver(
        &self,
        connection_handle: ConnectionHandle,
        connection_receiver: Receiver<ReliableNetworkMessage>,
        addr: String,
    ) -> ReliableSenderHandle {
        let (reliable_sender_handle, mut application_receiver) = ReliableSenderHandle::start(
            connection_handle,
            connection_receiver,
            self.delivery_timeout,
        )
        .await;
        let cloned = reliable_sender_handle.clone();
        let membership_addr = self.membership_addr.clone();

        let node_id = self.get_node_id();
        tokio::spawn(async move {
            while let Some(message) = application_receiver.recv().await {
                log::debug!("Application message received {:?}", message);
                match message.response_for_received(&node_id) {
                    Ok(Some(response)) => {
                        log::debug!("Sending Response {:?}", response);
                        let _ = cloned.send(response).await;
                    }
                    Ok(None) => {
                        log::info!("No response to send ")
                    }
                    Err(e) => {
                        log::info!("Error generating response {}", e)
                    }
                }
            }
            log::debug!("Connection clean up");
            let _ = membership_addr.send(RemoveMember { member: addr }).await;
        });
        reliable_sender_handle
    }
}

#[cfg(test)]
mod node_tests {
    use super::Node;

    #[actix::test]
    async fn it_should_return_well_formed_node_id() {
        let node = Node::new().await;
        assert_eq!(node.get_node_id(), "localhost");
    }
}
