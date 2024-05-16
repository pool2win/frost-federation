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

use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::mpsc,
};
use tokio_util::{
    bytes::Bytes,
    codec::{FramedRead, FramedWrite, LengthDelimitedCodec},
    sync::CancellationToken,
};

use crate::node::connection_reader::ConnectionReader;
use crate::node::connection_writer::ConnectionWriter;

use super::noise_handler::NoiseHandler;

#[derive(Debug)]
pub struct Connection {
    reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    read_channel: (mpsc::Sender<Bytes>, mpsc::Receiver<Bytes>),
    write_channel: (mpsc::Sender<Bytes>, mpsc::Receiver<Bytes>),
    requests_receiver: mpsc::Receiver<Bytes>,
}

impl Connection {
    /// Build a new connection struct to work with the TcpStream
    pub fn new(stream: TcpStream, requests_receiver: mpsc::Receiver<Bytes>) -> Self {
        // Setup a length delimted codec for Noise
        let (r, w) = stream.into_split();
        let framed_reader = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_read(r);
        let framed_writer = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_write(w);

        // Create a channel to buffer read processing
        let read_channel = mpsc::channel::<Bytes>(32);
        // Create a channel to buffer write processing
        let write_channel = mpsc::channel::<Bytes>(32);
        Connection {
            reader: framed_reader,
            writer: framed_writer,
            read_channel,
            write_channel,
            requests_receiver,
        }
    }

    pub async fn start(self, init: bool, pem_key: String) {
        let token = CancellationToken::new();
        let token_for_writer = token.clone();
        // let token_for_requests = token.clone();

        let mut noise_handler = NoiseHandler::new(
            self.read_channel.1,
            self.write_channel.0,
            self.requests_receiver,
            init,
            pem_key,
        );

        let mut connection_reader = ConnectionReader {
            send_channel: self.read_channel.0,
            framed_reader: self.reader,
            cancel_token: token,
        };
        tokio::spawn(async move {
            connection_reader.start().await;
        });

        let mut connection_writer = ConnectionWriter {
            receive_channel: self.write_channel.1,
            framed_writer: self.writer,
            cancel_token: token_for_writer,
        };
        tokio::spawn(async move {
            connection_writer.start().await;
        });

        noise_handler.run_handshake().await;
        log::info!("Handshake complete");
        noise_handler.start_transport().await;
        log::info!("Connection cleanup");
    }
}
