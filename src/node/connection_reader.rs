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

use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;

// Bring StreamExt in scope for access to `next` calls
use tokio_stream::StreamExt;

/// Read messages from the framed reader and enqueue it for noise and
/// higher layers to process.
pub struct ConnectionReader {
    pub send_channel: mpsc::Sender<Bytes>,
    pub framed_reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    pub cancel_token: CancellationToken,
}

impl ConnectionReader {
    pub async fn start(&mut self) {
        loop {
            if let Some(data) = self.framed_reader.next().await {
                match data {
                    Ok(data) => {
                        log::debug!("Received from network ... {:?}", data);
                        if let Err(e) = self.send_channel.send(data.freeze()).await {
                            log::debug!("Error en-queuing message: {}", e)
                        }
                    }
                    Err(e) => {
                        log::debug!("Error reading from channel {:?}", e);
                        log::info!("Closing connection");
                        self.cancel_token.cancel();
                        return;
                    }
                }
            }
        }
    }
}
