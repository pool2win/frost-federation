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

use super::connection::ConnectionHandle;

// TODO[pool2win]: Change this to XK once we have setup pubkey as node id
//
// We use 25519 instead of LN's choice of secp256k1 as
// rust Noise implementation doesn't yet support secp256k1
// noise_params: "Noise_XX_25519_ChaChaPoly_SHA256".parse().unwrap(),

// static PATTERN: &str = "Noise_NN_25519_ChaChaPoly_BLAKE2s";
static PATTERN: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const NOISE_MAX_MSG_LENGTH: usize = 65535;

/// Hold all the noise handshake and transport states
/// Noise handler reads and writes messages according to the noise
/// protocol used. The handler provides confidential and authenticated
/// channels between two peers.
pub struct NoiseHandler {
    handshake_state: Option<HandshakeState>,
    transport_state: Option<TransportState>,
    initiator: bool,
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
    pub fn new(init: bool, pem_key: String) -> Self {
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
            initiator: init,
        }
    }

    /// Switch to transport mode and drop the handshake state
    pub async fn start_transport(mut self) {
        let state = self.handshake_state.take();
        self.transport_state = Some(state.unwrap().into_transport_mode().unwrap());
    }

    /// Run the Noise handshake protocol for initiator or responder as
    /// the case may be
    pub async fn run_handshake(
        &mut self,
        connection_handle: ConnectionHandle,
        receiver: mpsc::Receiver<Bytes>,
    ) -> mpsc::Receiver<Bytes> {
        if self.initiator {
            self.initiator_handshake(connection_handle, receiver).await
        } else {
            self.responder_handshake(connection_handle, receiver).await
        }
    }

    /// Build a handshake message (using the handshake state)
    pub fn build_handshake_message(&mut self, message: &[u8]) -> Bytes {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .handshake_state
            .as_mut()
            .unwrap()
            .write_message(message, &mut buf)
            .unwrap();
        Bytes::from_iter(buf).slice(0..len)
    }

    /// Read handshake message (using the handshake state)
    pub fn read_handshake_message(&mut self, msg: Bytes) -> Bytes {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .handshake_state
            .as_mut()
            .unwrap()
            .read_message(&msg, &mut buf[..msg.len()])
            .unwrap();
        Bytes::from_iter(buf).slice(0..len)
    }

    /// Run initiator handshake steps. The steps here depend on the
    /// Noise protocol being used
    async fn initiator_handshake(
        &mut self,
        connection_handle: ConnectionHandle,
        mut receiver: mpsc::Receiver<Bytes>,
    ) -> mpsc::Receiver<Bytes> {
        let m1 = self.build_handshake_message(b"-> e");
        let _ = connection_handle.send(m1).await;

        let m2 = receiver.recv().await.unwrap();
        let _ = self.read_handshake_message(m2);

        let m3 = self.build_handshake_message(b"-> s, se");
        let _ = connection_handle.send(m3).await;
        log::info!("Noise channel ready");
        receiver
    }

    /// Run initiator handshake steps. The steps here depend on the
    /// Noise protocol being used
    async fn responder_handshake(
        &mut self,
        connection_handle: ConnectionHandle,
        mut receiver: mpsc::Receiver<Bytes>,
    ) -> mpsc::Receiver<Bytes> {
        let m1 = receiver.recv().await.unwrap();
        let _ = self.read_handshake_message(m1);

        let m2 = self.build_handshake_message(b"<- e, ee, s, es");
        let _ = connection_handle.send(m2).await;

        let m3 = receiver.recv().await.unwrap();
        let _ = self.read_handshake_message(m3);
        log::info!("Noise channel ready");
        receiver
    }
}
