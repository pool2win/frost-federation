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

#[mockall_double::double]
use self::echo_broadcast::EchoBroadcastHandle;
use self::{membership::MembershipHandle, protocol::Message};
use crate::node::echo_broadcast::service::EchoBroadcast;
use crate::node::noise_handler::{NoiseHandler, NoiseIO};
use crate::node::protocol::dkg::trigger::run_dkg_trigger;
use crate::node::protocol::init::initialize_handshake;
use crate::node::protocol::Protocol;
use crate::node::reliable_sender::service::ReliableSend;
use crate::node::reliable_sender::ReliableNetworkMessage;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use crate::node::state::State;
use commands::{Command, Commands};
#[mockall_double::double]
use connection::ConnectionHandle;
use frost_secp256k1 as frost;
use protocol::message_id_generator::MessageIdGenerator;
use std::error::Error;
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::{mpsc, oneshot},
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use tower::Layer;
use tower::ServiceExt;
use tracing::{debug, error, info};

pub mod commands;
mod connection;
mod echo_broadcast;
mod membership;
mod noise_handler;
mod protocol;
mod reliable_sender;
mod state;
mod test_helpers;

pub struct Node {
    pub seeds: Vec<String>,
    pub bind_address: String,
    pub static_key_pem: String,
    pub delivery_timeout: u64,
    pub(crate) state: State,
    pub(crate) echo_broadcast_handle: EchoBroadcastHandle,
    pub(crate) trigger_dkg_tx: mpsc::Sender<()>,
}

impl Node {
    /// Create a new Node with the given bind address and seeds
    pub async fn new(bind_address: String, seeds: Vec<String>) -> Self {
        let message_id_generator = MessageIdGenerator::new(bind_address.clone());
        let echo_broadcast_handle = EchoBroadcastHandle::start().await;
        let mut state = State::new(
            MembershipHandle::start(bind_address.clone()).await,
            message_id_generator,
        )
        .await;

        // We can send message on the channel from both sending and receiving tasks
        let (round_one_tx, mut round_one_rx) = mpsc::channel::<()>(2);
        state.round_one_tx = Some(round_one_tx);
        let (round_two_tx, mut round_two_rx) = mpsc::channel::<()>(2);
        state.round_two_tx = Some(round_two_tx);

        let (trigger_dkg_tx, mut trigger_dkg_rx) = mpsc::channel::<()>(1);

        let echo_broadcast_handle_dkg = echo_broadcast_handle.clone();
        let node_id = bind_address.clone();
        let state_dkg = state.clone();
        tokio::spawn(async move {
            run_dkg_trigger(
                node_id,
                state_dkg,
                echo_broadcast_handle_dkg,
                &mut round_one_rx,
                &mut round_two_rx,
                &mut trigger_dkg_rx,
            )
            .await;
        });

        Node {
            seeds,
            bind_address,
            static_key_pem: String::new(),
            delivery_timeout: 500,
            state,
            echo_broadcast_handle,
            trigger_dkg_tx,
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
    pub async fn start(
        &mut self,
        command_rx: mpsc::Receiver<Command>,
        accept_ready_tx: oneshot::Sender<()>,
    ) {
        debug!("Starting... {}", self.bind_address);
        if self.connect_to_seeds().await.is_err() {
            info!("Connecting to seeds failed.");
            return;
        }
        let listener = self.listen().await;
        if listener.is_err() {
            info!("Error starting listen");
        } else {
            let accept_task = self.start_accept(listener.unwrap(), accept_ready_tx);
            let command_task = self.start_command_loop(command_rx);
            // Stop node when accept returns or Command asks us to stop.
            tokio::select! {
                _ = accept_task => {
                    debug!("Accept finished");
                },
                _ = command_task => {
                    debug!("Command finished");
                }
            };
        }
    }

    /// Returns the bind address for all the nodes connected to this node
    pub async fn get_members(&self) -> Result<Vec<String>, Box<dyn Error + Send>> {
        match self.state.membership_handle.get_members().await {
            Ok(membership) => Ok(membership.into_keys().collect()),
            Err(_) => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Error getting members",
            ))),
        }
    }

    pub async fn get_dkg_public_key(
        &self,
    ) -> Result<Option<frost::keys::PublicKeyPackage>, Box<dyn Error + Send>> {
        match self.state.dkg_state.get_public_key_package().await {
            Ok(Some(pkg)) => Ok(Some(pkg)),
            Ok(None) => Ok(None),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Start listening
    pub async fn listen(&mut self) -> Result<TcpListener, Box<dyn Error>> {
        debug!("Start listen...");
        Ok(TcpListener::bind(self.bind_address.to_string()).await?)
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
    pub async fn start_accept(&self, listener: TcpListener, accept_ready_tx: oneshot::Sender<()>) {
        debug!("Start accepting...");
        let initiator = true;
        let _ = accept_ready_tx.send(());
        loop {
            debug!("Waiting on accept...");
            let (stream, socket_addr) = listener.accept().await.unwrap();
            debug!("Accept connection from {}", socket_addr);
            let (reader, writer) = self.build_reader_writer(stream);
            let noise = NoiseHandler::new(initiator, self.static_key_pem.clone());
            let (connection_handle, connection_receiver) =
                ConnectionHandle::start(reader, writer, noise, initiator).await;
            let (reliable_sender_handle, client_receiver) = self
                .start_reliable_sender_receiver(connection_handle, connection_receiver)
                .await;
            let _ = self
                .start_connection_event_loop(
                    socket_addr.to_string(),
                    reliable_sender_handle.clone(),
                    client_receiver,
                    self.echo_broadcast_handle.clone(),
                )
                .await;

            let node_id = self.get_node_id();
            let state = self.state.clone();
            let delivery_timeout = self.delivery_timeout;
            let reliable_sender = reliable_sender_handle.clone();
            initialize_handshake(node_id, state, reliable_sender, delivery_timeout).await;
        }
    }

    /// Connect to all peers and start reader writer tasks
    pub async fn connect_to_seeds(&mut self) -> Result<(), Box<dyn Error>> {
        debug!("Connecting to seeds...");
        let seeds = self.seeds.clone();
        let init = false;
        for seed in seeds.iter() {
            debug!("Connecting to seed {}", seed);
            if let Ok(stream) = TcpStream::connect(seed).await {
                let peer_addr = stream.peer_addr().unwrap();
                info!("Connected to {}", peer_addr);
                let (reader, writer) = self.build_reader_writer(stream);
                let noise = NoiseHandler::new(init, self.static_key_pem.clone());
                let (connection_handle, connection_receiver) =
                    ConnectionHandle::start(reader, writer, noise, init).await;
                let (reliable_sender_handle, client_receiver) = self
                    .start_reliable_sender_receiver(connection_handle, connection_receiver)
                    .await;
                let _ = self
                    .start_connection_event_loop(
                        peer_addr.to_string(),
                        reliable_sender_handle.clone(),
                        client_receiver,
                        self.echo_broadcast_handle.clone(),
                    )
                    .await;
            } else {
                debug!("Failed to connect to seed {}", seed);
                return Err("Failed to connect to seed".into());
            }
        }
        Ok(())
    }

    async fn start_reliable_sender_receiver(
        &self,
        connection_handle: ConnectionHandle,
        connection_receiver: mpsc::Receiver<ReliableNetworkMessage>,
    ) -> (ReliableSenderHandle, mpsc::Receiver<Message>) {
        let (client_receiver, reliable_sender_handle) =
            ReliableSenderHandle::start(connection_handle, connection_receiver).await;
        (reliable_sender_handle, client_receiver)
    }

    async fn start_connection_event_loop(
        &self,
        peer_addr: String,
        reliable_sender_handle: ReliableSenderHandle,
        mut client_receiver: mpsc::Receiver<Message>,
        echo_broadcast_handle: EchoBroadcastHandle,
    ) {
        let membership_handle = self.state.membership_handle.clone();
        let node_id = self.get_node_id();
        let timeout = self.delivery_timeout;
        let state = self.state.clone();

        tokio::spawn(async move {
            loop {
                let connection_message = client_receiver.recv().await;
                match connection_message {
                    Some(message) => match message {
                        Message::Unicast(unicast_message) => {
                            debug!("Unicast message received {:?}", unicast_message);
                            Node::respond_to_unicast_message(
                                node_id.clone(),
                                timeout,
                                Message::Unicast(unicast_message),
                                reliable_sender_handle.clone(),
                                state.clone(),
                            )
                            .await;
                        }
                        Message::Broadcast(broadcast_message, mid) => {
                            debug!("Broadcast message received {:?}", broadcast_message);
                            Node::respond_to_broadcast_message(
                                node_id.clone(),
                                timeout,
                                Message::Broadcast(broadcast_message, mid),
                                echo_broadcast_handle.clone(),
                                state.clone(),
                                reliable_sender_handle.clone(),
                            )
                            .await;
                        }
                        Message::Echo(broadcast_message, mid, peer_id) => {
                            info!("Received echo from network ...");
                            Node::respond_to_echo_message(
                                Message::Echo(broadcast_message, mid, peer_id),
                                echo_broadcast_handle.clone(),
                            )
                            .await;
                        }
                    },
                    _ => {
                        info!("Connection clean up");
                        if membership_handle.remove_member(peer_addr).await.is_ok() {
                            info!("Member removed");
                        }
                        return;
                    }
                }
            }
        });
    }

    async fn respond_to_unicast_message(
        node_id: String,
        timeout: u64,
        message: Message,
        reliable_sender_handle: ReliableSenderHandle,
        state: State,
    ) {
        tokio::spawn(async move {
            let protocol_service =
                Protocol::new(node_id.clone(), state, Some(reliable_sender_handle.clone()));
            let reliable_sender_service =
                ReliableSend::new(protocol_service, reliable_sender_handle);
            let timeout_layer =
                tower::timeout::TimeoutLayer::new(tokio::time::Duration::from_millis(timeout));
            let _ = timeout_layer
                .layer(reliable_sender_service)
                .oneshot(message)
                .await;
        });
    }

    async fn respond_to_broadcast_message(
        node_id: String,
        timeout: u64,
        message: Message,
        echo_broadcast_handle: EchoBroadcastHandle,
        state: State,
        reliable_sender_handle: ReliableSenderHandle,
    ) {
        tokio::spawn(async move {
            debug!("In respond to broadcast message {:?}", message);
            // TODO - This could cause the echo to go to new
            // members who didn't receive the initial
            // broadcast. Make sure they ignore such a message
            // as a benign error.
            let protocol_service = Protocol::new(
                node_id.clone(),
                state.clone(),
                Some(reliable_sender_handle.clone()),
            );
            let echo_broadcast_service =
                EchoBroadcast::new(protocol_service, echo_broadcast_handle, state, node_id);
            let timeout_layer =
                tower::timeout::TimeoutLayer::new(tokio::time::Duration::from_millis(timeout));
            if let Err(e) = timeout_layer
                .layer(echo_broadcast_service)
                .oneshot(message)
                .await
            {
                error!("Timeout error in broadcast message response: {}", e);
            }
        });
    }

    async fn respond_to_echo_message(message: Message, echo_broadcast_handle: EchoBroadcastHandle) {
        tokio::spawn(async move {
            let _ = echo_broadcast_handle.receive_echo(message).await;
        });
    }
}

#[cfg(test)]
mod node_tests {
    use super::Node;
    #[mockall_double::double]
    use crate::node::echo_broadcast::EchoBroadcastHandle;
    use crate::node::membership::MembershipHandle;
    use crate::node::protocol::message_id_generator::{MessageId, MessageIdGenerator};
    use crate::node::protocol::{dkg, Message, PingMessage};
    #[mockall_double::double]
    use crate::node::reliable_sender::ReliableSenderHandle;
    use futures::FutureExt;

    // We need to synchronize tests using mocks on Static fns like EchoBroadcastHandle::start
    // See here: https://docs.rs/mockall/latest/mockall/#static-methods
    use std::sync::Mutex;
    static MTX: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn it_should_return_well_formed_node_id() {
        let _m = MTX.lock();

        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone()
                .returning(|| EchoBroadcastHandle::default());
            mock
        });

        let node = Node::new("localhost".to_string(), vec!["localhost:6680".to_string()]).await;
        assert_eq!(node.get_node_id(), "localhost");
    }

    #[tokio::test]
    async fn it_should_create_node_with_config() {
        let _m = MTX.lock();

        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone()
                .returning(|| EchoBroadcastHandle::default());
            mock
        });

        let node = Node::new(
            "localhost:6880".to_string(),
            vec!["localhost:6881".to_string(), "localhost:6882".to_string()],
        )
        .await
        .static_key_pem("a key".to_string())
        .delivery_timeout(1000);

        assert_eq!(node.get_node_id(), "localhost:6880");
        assert_eq!(node.bind_address, "localhost:6880");
        assert_eq!(node.seeds[0], "localhost:6881");
        assert_eq!(node.seeds[1], "localhost:6882");
        assert_eq!(node.static_key_pem, "a key");
        assert_eq!(node.delivery_timeout, 1000);
    }

    #[tokio::test]
    async fn it_should_start_listen_without_error() {
        let _m = MTX.lock();

        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone()
                .returning(|| EchoBroadcastHandle::default());
            mock
        });

        let mut node = Node::new("localhost:6880".to_string(), vec![]).await;
        assert!(node.listen().await.is_ok());
    }

    #[tokio::test]
    async fn it_should_respond_to_unicast_messages() {
        let _m = MTX.lock();
        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(|| {
            let mut mock = EchoBroadcastHandle::default();
            mock.expect_clone()
                .returning(|| EchoBroadcastHandle::default());
            mock
        });

        let unicast_message: Message = PingMessage::default().into();
        let mut reliable_sender_handle = ReliableSenderHandle::default();

        reliable_sender_handle.expect_clone().return_once(|| {
            let mut cloned = ReliableSenderHandle::default();
            cloned
                .expect_send()
                .times(1)
                .return_once(|_| async { Ok(()) }.boxed());
            cloned
        });

        let membership_handle = MembershipHandle::start("local".into()).await;
        let state =
            super::State::new(membership_handle, MessageIdGenerator::new("local".into())).await;
        let res = Node::respond_to_unicast_message(
            "local".into(),
            100,
            unicast_message,
            reliable_sender_handle,
            state,
        )
        .await;
    }

    #[tokio::test]
    async fn it_should_respond_to_broadcast_messages() {
        let _m = MTX.lock();

        let ctx = EchoBroadcastHandle::start_context();
        ctx.expect().returning(EchoBroadcastHandle::default);

        let broadcast_message = crate::node::protocol::BroadcastProtocol::DKGRoundOnePackage(
            dkg::round_one::PackageMessage::new("local".into(), None),
        );
        let mut echo_broadcast_handle = EchoBroadcastHandle::default();
        echo_broadcast_handle.expect_clone().returning(|| {
            let mut mocked = EchoBroadcastHandle::default();
            mocked.expect_send().return_once(|_, _| Ok(()));
            mocked.expect_send_echo().return_once(|_, _| Ok(()));
            mocked
                .expect_track_received_broadcast()
                .return_once(|_, _, _| Ok(()));
            mocked
        });

        let membership_handle = MembershipHandle::start("local".into()).await;
        let state =
            super::State::new(membership_handle, MessageIdGenerator::new("local".into())).await;
        let reliable_sender_handle = ReliableSenderHandle::default();

        let res = Node::respond_to_broadcast_message(
            "local".into(),
            100,
            Message::Broadcast(broadcast_message, Some(MessageId(1))),
            echo_broadcast_handle,
            state,
            reliable_sender_handle,
        )
        .await;
    }
}
