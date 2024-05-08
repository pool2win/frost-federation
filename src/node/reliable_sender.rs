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

/// Reliably send a message by repeatedly sending it every timeout
/// period until an ACK is received.
pub struct ReliableSender {
    pub receive_channel: mpsc::Receiver<Bytes>,
    pub framed_writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    pub cancel_token: CancellationToken,
}

impl ReliableSender {
    //pub fn send(msg: Bytes) {}
    pub async fn start(&mut self, init: bool) {
        if init {
            let _ = self.framed_writer.send(Bytes::from("ping")).await;
        }
        loop {
            tokio::select! {
                Some(message) = self.receive_channel.recv() => {
                    let _ = self.framed_writer.send(message).await;
                },
                _ = self.cancel_token.cancelled() => {
                    return;
                }
            }
        }
    }
}
