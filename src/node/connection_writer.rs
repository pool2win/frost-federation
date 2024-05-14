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

use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::mpsc;
use tokio_util::bytes::Bytes;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;

// Bring SinkExt in scope for access to `send` calls
use futures::sink::SinkExt;

/// Send message over the noise connection
pub struct ConnectionWriter {
    pub receive_channel: mpsc::Receiver<Bytes>,
    pub framed_writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    pub cancel_token: CancellationToken,
}

impl ConnectionWriter {
    pub async fn start(&mut self) {
        loop {
            tokio::select! {
                Some(message) = self.receive_channel.recv() => {
                    log::debug!("Sending using framed writer {:?}", message);
                    if self.framed_writer.send(message).await.is_err() {
                        log::info!("Closing connection");
                        self.cancel_token.cancel();
                    }
                },
                _ = self.cancel_token.cancelled() => {
                    log::info!("Connection closed");
                    return;
                }
            }
        }
    }
}
