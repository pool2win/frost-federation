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

use super::connection::{ConnectionResult, ConnectionResultSender};
use super::membership::ReliableSenderMap;
use super::protocol::message_id_generator::MessageId;
use crate::node::protocol::Message;
#[mockall_double::double]
use crate::node::reliable_sender::ReliableSenderHandle;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

type EchosMap = HashMap<MessageId, HashMap<String, bool>>;

/// Message types for echo broadcast actor -> handle communication
pub(crate) enum EchoBroadcastMessage {
    Send {
        data: Message,
        message_id: MessageId,
        respond_to: ConnectionResultSender,
    },
    Echo {
        data: Message, // TODO: this can be taken away, so only the message_id acts as an ack/echo
        message_id: MessageId,
    },
}

/// Echo Broadcast Actor model implementation.
/// The actor receives messages from EchoBroadcastHandle
pub(crate) struct EchoBroadcastActor {
    reliable_sender_map: ReliableSenderMap,
    receiver: mpsc::Receiver<EchoBroadcastMessage>,
    echos: EchosMap,
    responders: HashMap<MessageId, ConnectionResultSender>,
}

impl EchoBroadcastActor {
    pub fn start(
        receiver: mpsc::Receiver<EchoBroadcastMessage>,
        reliable_sender_map: ReliableSenderMap,
    ) -> Self {
        Self {
            reliable_sender_map,
            receiver,
            echos: EchosMap::new(),
            responders: HashMap::new(),
        }
    }

    pub async fn handle_message(&mut self, message: EchoBroadcastMessage) {
        match message {
            EchoBroadcastMessage::Send {
                data,
                message_id,
                respond_to,
            } => {
                let _ = self.send_message(data, message_id, respond_to).await;
            }
            EchoBroadcastMessage::Echo { data, message_id } => {
                self.handle_received_echo(data, message_id).await;
            }
        }
    }

    /// Receives messages to echo broadcast, wait for echo messages
    /// from all members, on timeout, return an error to waiting
    /// receiver.
    ///
    /// Process:
    /// 1. Wait for echos from all members for the message sent
    /// 2. On timeout return error
    /// 3. On receiving echos from all members, return Ok() to waiting receiver
    pub async fn send_message(
        &mut self,
        data: Message,
        message_id: MessageId,
        respond_to: ConnectionResultSender,
    ) -> Result<(), Box<dyn Error + Sync + Send>> {
        let sender_id = data.get_sender_id();
        for (member, reliable_sender) in self.reliable_sender_map.iter_mut() {
            if reliable_sender.send(data.clone()).await.is_err() {
                log::debug!("Error in reliable sender send...");
                let _ = respond_to.send(Err("Error sending using reliable sender".into()));
                return Err("Error sending message using reliable sender".into());
            }
            let details = self.echos.entry(message_id.clone()).or_default();
            details.entry(member.clone()).or_insert(false);
        }

        // Update sender_id's echo to true
        self.echos
            .get_mut(&message_id)
            .unwrap()
            .insert(sender_id, true);

        self.responders.insert(message_id, respond_to);
        Ok(())
    }

    /// Check if echos have been received from all members for a given message identifier
    pub fn echo_received_for_all(&self, message_id: &MessageId) -> bool {
        self.echos
            .get(message_id)
            .unwrap()
            .iter()
            .all(|(_sender_id, status)| *status)
    }

    /// Add received echo from a sender to the list of echos received
    pub fn add_echo(&mut self, message_id: &MessageId, sender_id: String) {
        match self.echos.get_mut(message_id) {
            Some(echos) => {
                echos
                    .entry(sender_id)
                    .and_modify(|v| *v = true)
                    .or_insert(true);
            }
            None => {
                self.echos.insert(
                    message_id.clone(),
                    HashMap::<String, bool>::from([(sender_id, true)]),
                );
            }
        }
    }

    /// Handle Echo messages received for this message
    pub async fn handle_received_echo(&mut self, data: Message, message_id: MessageId) {
        let sender_id = data.get_sender_id();

        self.add_echo(&message_id, sender_id.clone());

        if self.echo_received_for_all(&message_id) {
            match self.responders.remove(&message_id) {
                Some(respond_to) => {
                    if respond_to.send(Ok(())).is_err() {
                        log::error!("Error responding on echo broadcast completion");
                    }
                }
                None => {
                    log::error!("No receivers for the confirmed echo broadcast");
                }
            }
        }
    }
}

use super::protocol::message_id_generator::MessageIdGenerator;

/// Handler for sending and confirming echo messages for a broadcast
/// Send using ReliableSenderHandle and then wait fro Echo message
///
/// Members list is copied into this struct so that we are only
/// waiting for echos from the parties that were members when the
/// broadcast was originally sent.
#[derive(Clone)]
pub(crate) struct EchoBroadcastHandle {
    sender: mpsc::Sender<EchoBroadcastMessage>,
    delivery_timeout: u64,
    message_id_generator: MessageIdGenerator,
}

async fn start_echo_broadcast(mut actor: EchoBroadcastActor) {
    while let Some(message) = actor.receiver.recv().await {
        actor.handle_message(message).await;
    }
}

/// Handle for the echo broadcast actor
impl EchoBroadcastHandle {
    pub fn start(
        message_id_generator: MessageIdGenerator,
        delivery_timeout: u64,
        reliable_senders_map: ReliableSenderMap,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(32);
        let actor = EchoBroadcastActor::start(receiver, reliable_senders_map);
        tokio::spawn(start_echo_broadcast(actor));

        Self {
            sender,
            delivery_timeout,
            message_id_generator,
        }
    }

    /// Keep the same signature to send, so we can convert that into a Trait later if we want.
    pub async fn send(&self, message: Message) -> ConnectionResult<()> {
        let (sender_from_actor, receiver_from_actor) = oneshot::channel();
        let msg = EchoBroadcastMessage::Send {
            data: message,
            message_id: self.message_id_generator.next(),
            respond_to: sender_from_actor,
        };
        if self.sender.send(msg).await.is_err() {
            log::debug!("Returning send error...");
            return Err("Connection error".into());
        }
        let result = timeout(
            Duration::from_millis(self.delivery_timeout),
            receiver_from_actor,
        )
        .await;
        if result??.is_err() {
            log::debug!("Returning time out error...");
            Err("Broadcast timed out".into())
        } else {
            Ok(())
        }
    }
}

// #[cfg(test)]
// mod echo_broadcast_handler_tests {
//     use std::time::SystemTime;

//     use crate::node::protocol::HeartbeatMessage;

//     use super::*;

//     #[tokio::test]
//     async fn it_should_return_timeout_error_on_send_without_echos() {
//         let delivery_timeout = 10;
//         let mut mock_reliable_sender = MockReliableSender::new();
//         mock_reliable_sender.expect_send().return_once(|_| Ok(()));
//         let reliable_senders_map = HashMap::from([("a".to_string(), mock_reliable_sender)]);
//         let message_id_generator = MessageIdGenerator::new("localhost".to_string());

//         let handle = EchoBroadcastHandle::start(
//             message_id_generator,
//             delivery_timeout,
//             reliable_senders_map,
//         );

//         let message = Message::Heartbeat(HeartbeatMessage {
//             sender_id: "localhost".into(),
//             time: SystemTime::now(),
//         });

//         let result = handle.send(message).await;
//         assert!(result.is_err());
//     }
// }

// #[cfg(test)]
// mod echo_broadcast_actor_tests {
//     use super::*;

//     // async fn build_reliable_sender_handle_with_ok_response() -> ReliableSenderHandle {
//     //     let (tx, mut rx) = mpsc::channel::<ReliableMessage>(10);
//     //     tokio::spawn(async move {
//     //         while let Some(m) = rx.recv().await {
//     //             //println!("Received {:?}", m);
//     //         }
//     //     });
//     //     ReliableSenderHandle { sender: tx }
//     // }

//     #[tokio::test]
//     async fn it_should_create_actor_with_echos_setup() {
//         let (_sender, receiver) = mpsc::channel(32);
//         let reliable_sender = ReliableSenderHandle::default();
//         let reliable_senders_map = HashMap::from([("a".to_string(), reliable_sender)]);
//         let actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//         assert_eq!(actor.echos.keys().count(), 0);
//     }

// #[tokio::test]
// async fn it_should_handle_send_message_with_ok_from_reliable_sender() {
//     let (_sender, receiver) = mpsc::channel(32);
//     let (respond_to, waiting_for_response) = oneshot::channel();

//     let reliable_sender = build_reliable_sender_handle_with_ok_response().await;

//     let reliable_senders_map = HashMap::from([("a".to_string(), reliable_sender)]);

//     let mut actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//     let data = Message::Ping(PingMessage {
//         sender_id: "localhost".to_string(),
//         message: "ping".to_string(),
//     });

//     let result = actor.send_message(data, MessageId(0), respond_to).await;
//     assert!(result.is_ok());
// }

// #[tokio::test]
// async fn it_should_handle_send_message_with_error_from_reliable_sender() {
//     let (_sender, receiver) = mpsc::channel(32);
//     let (respond_to, waiting_for_response) = oneshot::channel();

//     let reliable_sender = build_reliable_sender_handle_with_ok_response().await;

//     let reliable_senders_map = HashMap::from([("a".to_string(), reliable_sender)]);

//     let mut actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//     let data = Message::Ping(PingMessage {
//         sender_id: "localhost".to_string(),
//         message: "ping".to_string(),
//     });

//     let result = actor.send_message(data, MessageId(0), respond_to).await;
//     assert!(result.is_err());
// }

//     #[tokio::test]
//     async fn it_should_send_handle_errors_if_echos_time_out() {
//         let mut first_reliable_sender_handle = MockReliableSender::new();
//         let mut second_reliable_sender_handle = MockReliableSender::new();

//         let msg = Message::Ping(PingMessage {
//             sender_id: "localhost".to_string(),
//             message: "ping".to_string(),
//         });

//         first_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));
//         second_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));

//         let reliable_senders_map = HashMap::from([
//             ("a".to_string(), first_reliable_sender_handle),
//             ("b".to_string(), second_reliable_sender_handle),
//         ]);
//         let message_id_generator = MessageIdGenerator::new("localhost".to_string());
//         let delivery_timeout = 1000;

//         let echo_bcast = EchoBroadcastHandle::start(
//             message_id_generator,
//             delivery_timeout,
//             reliable_senders_map,
//         );

//         let result = echo_bcast.send(msg).await;
//         assert_eq!(
//             result.as_ref().unwrap_err().to_string(),
//             "deadline has elapsed"
//         );
//         assert!(result.is_err());
//     }

//     #[tokio::test]
//     async fn it_should_send_handle_errors_if_reliable_sender_returns_error() {
//         let mut first_reliable_sender_handle = MockReliableSender::new();
//         let mut second_reliable_sender_handle = MockReliableSender::new();

//         let msg = Message::Ping(PingMessage {
//             sender_id: "localhost".to_string(),
//             message: "ping".to_string(),
//         });

//         first_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));
//         second_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Err("Error sending message".into()));

//         let reliable_senders_map = HashMap::from([
//             ("a".to_string(), first_reliable_sender_handle),
//             ("b".to_string(), second_reliable_sender_handle),
//         ]);
//         let message_id_generator = MessageIdGenerator::new("localhost".to_string());
//         let delivery_timeout = 1000;

//         let echo_bcast = EchoBroadcastHandle::start(
//             message_id_generator,
//             delivery_timeout,
//             reliable_senders_map,
//         );

//         let result = echo_bcast.send(msg).await;
//         assert!(result.is_err());
//         assert_eq!(
//             result.as_ref().unwrap_err().to_string(),
//             "Broadcast timed out"
//         );
//     }

//     #[tokio::test]
//     async fn it_should_handle_echo_message_if_waiting_for_echos() {
//         let mut first_reliable_sender_handle = MockReliableSender::new();
//         let mut second_reliable_sender_handle = MockReliableSender::new();

//         first_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));
//         second_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));

//         let reliable_senders_map = HashMap::from([
//             ("a".to_string(), first_reliable_sender_handle),
//             ("b".to_string(), second_reliable_sender_handle),
//         ]);

//         let (_sender, receiver) = mpsc::channel(32);
//         let mut echo_bcast_actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//         let ping_msg = Message::Ping(PingMessage {
//             sender_id: "localhost".to_string(),
//             message: "ping".to_string(),
//         });

//         let msg = EchoBroadcastMessage::Echo {
//             data: ping_msg,
//             message_id: MessageId(1),
//         };

//         let _result = echo_bcast_actor.handle_message(msg).await;
//         assert_eq!(echo_bcast_actor.echos.len(), 1);
//         assert_eq!(echo_bcast_actor.echos.get(&MessageId(1)).unwrap().len(), 1);
//         assert!(echo_bcast_actor
//             .echos
//             .get(&MessageId(1))
//             .unwrap()
//             .get("localhost")
//             .is_some());
//     }

//     #[tokio::test]
//     async fn it_should_add_echo_in_various_cases() {
//         let _ = env_logger::builder().is_test(true).try_init();

//         let mut first_reliable_sender_handle = MockReliableSender::new();
//         let mut second_reliable_sender_handle = MockReliableSender::new();

//         first_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));
//         second_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));

//         let reliable_senders_map = HashMap::from([
//             ("b".to_string(), first_reliable_sender_handle),
//             ("c".to_string(), second_reliable_sender_handle),
//         ]);

//         let (_sender, receiver) = mpsc::channel(32);
//         let mut echo_bcast_actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//         echo_bcast_actor.add_echo(&MessageId(1), "b".to_string());
//         echo_bcast_actor.add_echo(&MessageId(1), "c".to_string());

//         assert_eq!(echo_bcast_actor.echos.len(), 1);
//         assert_eq!(echo_bcast_actor.echos.get(&MessageId(1)).unwrap().len(), 2);

//         // try to add same echo again
//         echo_bcast_actor.add_echo(&MessageId(1), "b".to_string());
//         assert_eq!(echo_bcast_actor.echos.get(&MessageId(1)).unwrap().len(), 2);
//     }

//     #[tokio::test]
//     async fn it_should_handle_echo_before_all_received_will_add_echo() {
//         let mut first_reliable_sender_handle = MockReliableSender::new();
//         let mut second_reliable_sender_handle = MockReliableSender::new();

//         first_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));
//         second_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));

//         let reliable_senders_map = HashMap::from([
//             ("a".to_string(), first_reliable_sender_handle),
//             ("b".to_string(), second_reliable_sender_handle),
//         ]);

//         let (_sender, receiver) = mpsc::channel(32);
//         let mut echo_bcast_actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//         let ping_msg = Message::Ping(PingMessage {
//             sender_id: "localhost".to_string(),
//             message: "ping".to_string(),
//         });

//         echo_bcast_actor
//             .handle_received_echo(ping_msg, MessageId(1))
//             .await;

//         assert_eq!(echo_bcast_actor.echos.len(), 1);
//         assert_eq!(echo_bcast_actor.echos.get(&MessageId(1)).unwrap().len(), 1);
//     }

//     #[tokio::test]
//     async fn it_should_handle_echo_to_manage_all_received_case() {
//         let _ = env_logger::builder().is_test(true).try_init();

//         let mut first_reliable_sender_handle = MockReliableSender::new();
//         let mut second_reliable_sender_handle = MockReliableSender::new();

//         first_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));
//         second_reliable_sender_handle
//             .expect_send()
//             .return_once(|_| Ok(()));

//         let reliable_senders_map = HashMap::from([
//             ("b".to_string(), first_reliable_sender_handle),
//             ("c".to_string(), second_reliable_sender_handle),
//         ]);

//         let (_sender, receiver) = mpsc::channel(32);
//         let (msg_sender, msg_receiver) = oneshot::channel();
//         let mut echo_bcast_actor = EchoBroadcastActor::start(receiver, reliable_senders_map);

//         let ping_msg = Message::Ping(PingMessage {
//             sender_id: "localhost".to_string(),
//             message: "ping".to_string(),
//         });

//         let result = echo_bcast_actor
//             .send_message(ping_msg.clone(), MessageId(1), msg_sender)
//             .await;

//         assert!(result.is_ok());
//         assert_eq!(echo_bcast_actor.responders.len(), 1);

//         let echo_b = Message::Ping(PingMessage {
//             sender_id: "b".to_string(),
//             message: "ping".to_string(),
//         });

//         let echo_c = Message::Ping(PingMessage {
//             sender_id: "c".to_string(),
//             message: "ping".to_string(),
//         });

//         echo_bcast_actor
//             .handle_received_echo(echo_b, MessageId(1))
//             .await;

//         echo_bcast_actor
//             .handle_received_echo(echo_c, MessageId(1))
//             .await;

//         assert!(msg_receiver.await.is_ok());
//     }
// }
