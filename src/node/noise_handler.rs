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
use tokio_util::bytes::Bytes;

// TODO[pool2win]: Change this to XK once we have setup pubkey as node id
//
// We use 25519 instead of LN's choice of secp256k1 as
// rust Noise implementation doesn't yet support secp256k1

static PATTERN: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
const NOISE_MAX_MSG_LENGTH: usize = 65535;

fn build_keypair(key: &str) -> Result<Keypair, snow::Error> {
    let decoded = SigningKey::from_pkcs8_pem(key).unwrap();
    let keypair_bytes = decoded.to_keypair_bytes();
    Ok(Keypair {
        private: keypair_bytes[..SECRET_KEY_LENGTH].to_vec(),
        public: keypair_bytes[SECRET_KEY_LENGTH..].to_vec(),
    })
}

#[mockall::automock]
pub trait NoiseIO {
    fn new(init: bool, pem_key: String) -> Self;
    fn start_transport(&mut self);
    fn build_handshake_message(&mut self, message: &[u8]) -> Bytes;
    fn read_handshake_message(&mut self, msg: Bytes) -> Bytes;
    fn build_transport_message(&mut self, message: &[u8]) -> Bytes;
    fn read_transport_message(&mut self, msg: Bytes) -> Bytes;
}

/// Hold all the noise handshake and transport states
/// Noise handler reads and writes messages according to the noise
/// protocol used. The handler provides confidential and authenticated
/// channels between two peers.
#[derive(Debug)]
pub struct NoiseHandler {
    handshake_state: Option<HandshakeState>,
    transport_state: Option<TransportState>,
    initiator: bool,
}

impl NoiseIO for NoiseHandler {
    fn new(init: bool, pem_key: String) -> Self {
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
    fn start_transport(&mut self) {
        let state = self.handshake_state.take();
        self.transport_state = Some(state.unwrap().into_transport_mode().unwrap());
    }

    /// Build a handshake message (using the handshake state)
    fn build_handshake_message(&mut self, message: &[u8]) -> Bytes {
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
    fn read_handshake_message(&mut self, msg: Bytes) -> Bytes {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .handshake_state
            .as_mut()
            .unwrap()
            .read_message(&msg, &mut buf[..msg.len()])
            .unwrap();
        Bytes::from_iter(buf).slice(0..len)
    }

    /// Build a handshake message (using the handshake state)
    fn build_transport_message(&mut self, message: &[u8]) -> Bytes {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .transport_state
            .as_mut()
            .unwrap()
            .write_message(message, &mut buf)
            .unwrap();
        Bytes::from_iter(buf).slice(0..len)
    }

    /// Read handshake message (using the handshake state)
    fn read_transport_message(&mut self, msg: Bytes) -> Bytes {
        let mut buf = [0u8; NOISE_MAX_MSG_LENGTH];
        let len = self
            .transport_state
            .as_mut()
            .unwrap()
            .read_message(&msg, &mut buf[..msg.len()])
            .unwrap();
        Bytes::from_iter(buf).slice(0..len)
    }
}

/// handshake functions are in a separate module. This makes it
/// possible to mock them using mockall for unit testing.
pub mod handshake {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use tokio_util::bytes::{Bytes, BytesMut};

    /// Run the Noise handshake protocol for initiator or responder as
    /// the case may be
    pub async fn run_handshake<R, W>(
        noise: &mut impl NoiseIO,
        initiator: bool,
        reader: R,
        writer: W,
    ) where
        R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
        W: SinkExt<Bytes> + Unpin,
    {
        if initiator {
            initiator_handshake(noise, reader, writer).await
        } else {
            responder_handshake(noise, reader, writer).await
        };
        noise.start_transport();
    }

    /// Run initiator handshake steps. The steps here depend on the
    /// Noise protocol being used
    pub async fn initiator_handshake<R, W>(noise: &mut impl NoiseIO, mut reader: R, mut writer: W)
    where
        R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
        W: SinkExt<Bytes> + Unpin,
    {
        let m1 = noise.build_handshake_message(b"1");
        log::debug!("m1 : {:?}", m1.clone());
        let _ = writer.send(m1).await;

        let m2 = reader.next().await.unwrap().unwrap().freeze();
        log::debug!("m2 : {:?}", m2.clone());
        let _ = noise.read_handshake_message(m2);

        let m3 = noise.build_handshake_message(b"-> s, se");
        log::debug!("m3 : {:?}", m3.clone());
        let _ = writer.send(m3).await;

        log::info!("Noise channel ready");
    }

    /// Run initiator handshake steps. The steps here depend on the
    /// Noise protocol being used
    async fn responder_handshake<R, W>(noise: &mut impl NoiseIO, mut reader: R, mut writer: W)
    where
        R: StreamExt<Item = Result<BytesMut, std::io::Error>> + Unpin,
        W: SinkExt<Bytes> + Unpin,
    {
        let m1 = reader.next().await.unwrap().unwrap().freeze();
        log::debug!("m1 : {:?}", m1.clone());
        let _ = noise.read_handshake_message(m1);

        let m2 = noise.build_handshake_message(b"<- e, ee, s, es");
        log::debug!("m2 : {:?}", m2.clone());
        let _ = writer.send(m2).await;

        let m3 = reader.next().await.unwrap().unwrap().freeze();
        log::debug!("m3 : {:?}", m3.clone());
        let _ = noise.read_handshake_message(m3);

        log::info!("Noise channel ready");
    }
}

#[cfg(test)]
mod tests {

    use super::{handshake, MockNoiseIO, NoiseHandler, NoiseIO};
    use tokio_util::bytes::{Bytes, BytesMut};

    static TEST_KEY: &str = "
-----BEGIN PRIVATE KEY-----
MFECAQEwBQYDK2VwBCIEIJ7pILqR7yBPsVuysfGyofjOEm19skmtqcJYWiVwjKH1
gSEA68zeZuy7PMMQC9ECPmWqDl5AOFj5bi243F823ZVWtXY=
-----END PRIVATE KEY-----
";

    fn build_initiator_responder() -> (NoiseHandler, NoiseHandler) {
        let mut initiator = NoiseHandler::new(true, TEST_KEY.to_string());
        let mut responder = NoiseHandler::new(false, TEST_KEY.to_string());

        let m1 = initiator.build_handshake_message(b"");
        let _ = responder.read_handshake_message(m1);

        let m2 = responder.build_handshake_message(b"");
        let _ = initiator.read_handshake_message(m2);

        let m3 = initiator.build_handshake_message(b"");
        let _ = responder.read_handshake_message(m3);

        initiator.start_transport();
        responder.start_transport();

        (initiator, responder)
    }

    #[test]
    fn it_builds_noise_handler() {
        let handler = NoiseHandler::new(true, TEST_KEY.to_string());
        assert!(handler.initiator);
        assert!(handler.handshake_state.is_some());
        assert!(handler.transport_state.is_none());
    }

    #[test]
    fn it_should_build_and_read_handshake_messages() {
        let mut handler = NoiseHandler::new(true, TEST_KEY.to_string());
        let noise_message: Bytes = handler.build_handshake_message(b"test bytes");
        let len = noise_message.len();
        assert_eq!(&noise_message[(len - 10)..], b"test bytes");

        let mut responder = NoiseHandler::new(false, TEST_KEY.to_string());
        let read_message: Bytes = responder.read_handshake_message(noise_message);
        let len = read_message.len();
        assert_eq!(&read_message[(len - 10)..], b"test bytes");
    }

    #[test]
    fn it_should_run_handshake_and_transition_to_transport_state() {
        let (initiator, responder) = build_initiator_responder();
        assert!(initiator.handshake_state.is_none());
        assert!(initiator.transport_state.is_some());

        assert!(responder.handshake_state.is_none());
        assert!(responder.transport_state.is_some());
    }

    #[test]
    fn it_should_build_and_read_transport_messages() {
        let (mut initiator, mut responder) = build_initiator_responder();
        let m = initiator.build_transport_message(b"hello world");
        let m_read = responder.read_transport_message(m);

        assert_eq!(m_read, Bytes::from("hello world"));

        let m = responder.build_transport_message(b"goodbye");
        let m_read = initiator.read_transport_message(m);

        assert_eq!(m_read, Bytes::from("goodbye"));
    }

    #[tokio::test]
    async fn it_should_run_noise_handshake_protocol_as_initiator() {
        let mut noise_mock = MockNoiseIO::default();

        let read_buffer: Vec<Result<BytesMut, std::io::Error>> = vec![Ok(BytesMut::from("m1"))];
        let initiator_reader = futures::stream::iter(read_buffer);
        let initiator_writer: Vec<Bytes> = vec![];

        noise_mock
            .expect_build_handshake_message()
            .return_const(Bytes::from("m1"));

        noise_mock
            .expect_read_handshake_message()
            .return_const(Bytes::from("m2"));

        noise_mock.expect_start_transport().return_const(());

        let _ = handshake::run_handshake(&mut noise_mock, true, initiator_reader, initiator_writer)
            .await;
    }

    #[tokio::test]
    async fn it_should_run_noise_handshake_protocol_as_responder() {
        let mut noise_mock = MockNoiseIO::default();

        let read_buffer: Vec<Result<BytesMut, std::io::Error>> =
            vec![Ok(BytesMut::from("m1")), Ok(BytesMut::from("m3"))];
        let responder_reader = futures::stream::iter(read_buffer);
        let responder_writer: Vec<Bytes> = vec![];

        noise_mock
            .expect_build_handshake_message()
            .return_const(Bytes::from("m1"));

        noise_mock
            .expect_read_handshake_message()
            .return_const(Bytes::from("m2"));

        noise_mock.expect_start_transport().return_const(());

        let _ =
            handshake::run_handshake(&mut noise_mock, false, responder_reader, responder_writer)
                .await;
    }
}
