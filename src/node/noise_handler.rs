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
use ed25519_dalek::SigningKey;
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

pub struct NoiseHandler {
    /// Channel Sender that is consumed by Connection to write to
    /// framed writer
    write_channel_sx: mpsc::Sender<Bytes>,
    /// Channel Receiver that consumes messages produced by
    /// Connection's framed reader
    read_channel_rx: mpsc::Receiver<Bytes>,
    keypair: Keypair,
    handshake_state: HandshakeState,
    transport_state: Option<TransportState>,
    initiator: bool,
    static_key: SigningKey,
}

impl NoiseHandler {
    pub fn new(
        read_channel_rx: mpsc::Receiver<Bytes>,
        write_channel_sx: mpsc::Sender<Bytes>,
        init: bool,
        pem_key: String,
    ) -> Self {
        let parsed_pattern = PATTERN.parse().unwrap();
        let mut builder = snow::Builder::new(parsed_pattern);
        // TODO: Read this static key from config file. Restarts should use the same key.
        let keypair = builder.generate_keypair().unwrap();
        builder = builder.local_private_key(&keypair.private);
        let handshake_state = if init {
            builder.build_initiator().unwrap()
        } else {
            builder.build_responder().unwrap()
        };
        let decoded_private_key = SigningKey::from_pkcs8_pem(pem_key.as_str()).unwrap();
        log::debug!("Using private key {:?}", decoded_private_key);
        NoiseHandler {
            handshake_state,
            transport_state: None,
            keypair,
            read_channel_rx,
            write_channel_sx,
            initiator: init,
            static_key: decoded_private_key,
        }
    }

    pub async fn run_handshake(self) {
        if self.initiator {
            self.initiator_handshake().await;
        } else {
            self.responder_handshake().await;
        }
    }

    async fn send_handshake_message(&mut self, message: &[u8]) {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .handshake_state
            .write_message(message, &mut buf)
            .unwrap();
        let _ = self
            .write_channel_sx
            .send(Bytes::from_iter(buf).slice(0..len))
            .await;
    }

    async fn read_handshake_message(&mut self) {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let msg = self.read_channel_rx.recv().await.unwrap();
        let _len = self
            .handshake_state
            .read_message(&msg, &mut buf[..msg.len()])
            .unwrap();
    }

    async fn initiator_handshake(mut self) {
        // -> e
        self.send_handshake_message(b"").await;

        self.read_handshake_message().await;
        // // -> s, se
        self.send_handshake_message(b"").await;

        self.transport_state = Some(self.handshake_state.into_transport_mode().unwrap());
    }

    async fn responder_handshake(mut self) {
        self.read_handshake_message().await;
        // <- e, ee, s, es
        self.send_handshake_message(b"").await;

        // read -> s, se to complete handshake
        self.read_handshake_message().await;

        self.transport_state = Some(self.handshake_state.into_transport_mode().unwrap());
    }
}
