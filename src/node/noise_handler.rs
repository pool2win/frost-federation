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

use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::{SigningKey, SECRET_KEY_LENGTH};
use snow::{HandshakeState, Keypair, TransportState};
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;

// TODO[pool2win]: Change this to XK once we have setup pubkey as node id
//
// We use 25519 instead of LN's choice of secp256k1 as
// rust Noise implementation doesn't yet support secp256k1
// noise_params: "Noise_XX_25519_ChaChaPoly_SHA256".parse().unwrap(),

// static PATTERN: &str = "Noise_NN_25519_ChaChaPoly_BLAKE2s";
static PATTERN: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const NOISE_MAX_MSG_LENGTH: usize = 65535;

/// Hold all the noise handshake and transport states as well as the
/// various channels to send/receive messages from protocols and
/// socket stream
///
/// Noise handler reads and writes messages according to the noise
/// protocol used. The handler provides confidential and authenticated
/// channels between two peers.
pub struct NoiseHandler {
    /// Channel Sender that is consumed by Connection to write to
    /// framed writer
    write_channel_sx: mpsc::Sender<Bytes>,
    /// Channel Receiver that consumes messages produced by
    /// Connection's framed reader
    read_channel_rx: mpsc::Receiver<Bytes>,
    handshake_state: Option<HandshakeState>,
    transport_state: Option<TransportState>,
    initiator: bool,
    requests_receiver: mpsc::Receiver<Bytes>,
}

fn build_keypair(key: &str) -> Result<Keypair, snow::Error> {
    let decoded = SigningKey::from_pkcs8_pem(key).unwrap();
    let keypair_bytes = decoded.to_keypair_bytes();
    Ok(Keypair {
        private: keypair_bytes[..SECRET_KEY_LENGTH].to_vec(),
        public: keypair_bytes[SECRET_KEY_LENGTH..].to_vec(),
    })
}

impl NoiseHandler {
    pub fn new(
        read_channel_rx: mpsc::Receiver<Bytes>,
        write_channel_sx: mpsc::Sender<Bytes>,
        requests_receiver: mpsc::Receiver<Bytes>,
        init: bool,
        pem_key: String,
    ) -> Self {
        let parsed_pattern = PATTERN.parse().unwrap();
        let mut builder = snow::Builder::new(parsed_pattern);
        let keypair = build_keypair(pem_key.as_str()).unwrap();
        builder = builder.local_private_key(&keypair.private);
        let handshake_state = if init {
            Some(builder.build_initiator().unwrap())
        } else {
            Some(builder.build_responder().unwrap())
        };
        NoiseHandler {
            handshake_state,
            transport_state: None,
            read_channel_rx,
            write_channel_sx,
            initiator: init,
            requests_receiver,
        }
    }

    /// Switch to transport mode and drop the handshake state
    pub async fn start_transport(mut self) {
        let state = self.handshake_state.take();
        self.transport_state = Some(state.unwrap().into_transport_mode().unwrap());
        self.start_receiving_requests().await;
    }

    /// Start receiving messages from higher layers/protocols to send
    /// to the peer
    pub async fn start_receiving_requests(mut self) {
        loop {
            if let Some(message) = self.requests_receiver.recv().await {
                log::debug!("Sending message {:?}", message);
                let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
                let len = self
                    .transport_state
                    .as_mut()
                    .unwrap()
                    .write_message(&message, &mut buf)
                    .unwrap();
                let _ = self
                    .write_channel_sx
                    .send(Bytes::from_iter(buf).slice(0..len))
                    .await;
            }
        }
    }

    /// Run the Noise handshake protocol for initiator or responder as
    /// the case may be
    pub async fn run_handshake(&mut self) {
        if self.initiator {
            self.initiator_handshake().await;
        } else {
            self.responder_handshake().await;
        }
    }

    /// Send a handshake message (using the handshake state)
    async fn send_handshake_message(&mut self, message: &[u8]) {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .handshake_state
            .as_mut()
            .unwrap()
            .write_message(message, &mut buf)
            .unwrap();
        let _ = self
            .write_channel_sx
            .send(Bytes::from_iter(buf).slice(0..len))
            .await;
    }

    /// Read handshake message (using the handshake state)
    async fn read_handshake_message(&mut self) {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let msg = self.read_channel_rx.recv().await.unwrap();
        let _len = self
            .handshake_state
            .as_mut()
            .unwrap()
            .read_message(&msg, &mut buf[..msg.len()])
            .unwrap();
    }

    /// Run initiator handshake steps. The steps here depend on the
    /// Noise protocol being used
    async fn initiator_handshake(&mut self) {
        self.send_handshake_message(b"").await; // send -> e
        self.read_handshake_message().await; // receive <- e, ee, s, es
        self.send_handshake_message(b"").await; // send -> s, se
    }

    /// Run initiator handshake steps. The steps here depend on the
    /// Noise protocol being used
    async fn responder_handshake(&mut self) {
        self.read_handshake_message().await; // receive -> e
        self.send_handshake_message(b"").await; // send <- e, ee, s, es
        self.read_handshake_message().await; // receive -> s, se
    }
}
