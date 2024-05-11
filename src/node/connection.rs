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

use crate::node::noise_reader::NoiseReader;
use crate::node::noise_writer::NoiseWriter;

use super::noise_handler::NoiseHandler;

#[derive(Debug)]
pub struct Connection {
    reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    read_channel: (mpsc::Sender<Bytes>, mpsc::Receiver<Bytes>),
    write_channel: (mpsc::Sender<Bytes>, mpsc::Receiver<Bytes>),
}

impl Connection {
    /// Build a new connection struct to work with the TcpStream
    pub fn new(stream: TcpStream) -> Self {
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
        }
    }

    pub async fn start(self, init: bool) {
        let token = CancellationToken::new();
        let cloned_token = token.clone();

        let noise_handler = NoiseHandler::new(self.read_channel.1, self.write_channel.0, init);

        let mut noise_reader = NoiseReader {
            send_channel: self.read_channel.0,
            framed_reader: self.reader,
            cancel_token: token,
        };
        tokio::spawn(async move {
            noise_reader.start().await;
        });

        let mut noise_writer = NoiseWriter {
            receive_channel: self.write_channel.1,
            framed_writer: self.writer,
            cancel_token: cloned_token,
        };
        tokio::spawn(async move {
            noise_writer.start().await;
        });

        noise_handler.run_handshake().await;
    }
}
