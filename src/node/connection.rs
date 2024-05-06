use futures::sink::SinkExt;
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::mpsc,
};
use tokio_stream::StreamExt;
use tokio_util::{
    bytes::Bytes,
    codec::{FramedRead, FramedWrite, LengthDelimitedCodec},
};

#[derive(Debug)]
pub struct Connection {
    reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    send_channel: mpsc::Sender<Bytes>,
    receive_channel: mpsc::Receiver<Bytes>,
}

impl Connection {
    /// Build a new connection struct to work with the TcpStream
    pub fn new(stream: TcpStream) -> Self {
        // Setup a length delimted codec for Noise
        let (r, w) = stream.into_split();
        let reader = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_read(r);
        let writer = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(2)
            .length_adjustment(0)
            .new_write(w);

        // Create a channel to buffer read/write processing
        let buffering_channel = mpsc::channel::<Bytes>(32);
        Connection {
            reader,
            writer,
            send_channel: buffering_channel.0,
            receive_channel: buffering_channel.1,
        }
    }

    pub async fn start(self, init: bool) {
        tokio::spawn(async move {
            start_reader(self.reader, self.send_channel).await;
        });
        tokio::spawn(async move {
            start_writer(self.writer, self.receive_channel, init).await;
        });
    }
}

pub async fn start_reader(
    mut reader: FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
    wx: mpsc::Sender<Bytes>,
) {
    loop {
        if let Some(data) = reader.next().await {
            match data {
                Ok(data) => {
                    log::debug!("Received ... {:?}", data);
                    if let Err(e) = wx.send(data.freeze()).await {
                        log::debug!("Error en-queuing message: {}", e)
                    }
                }
                Err(e) => log::info!("Error receiving data {:?}", e),
            }
        }
    }
}

pub async fn start_writer(
    mut writer: FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
    mut rx: mpsc::Receiver<Bytes>,
    init: bool,
) {
    if init {
        let _ = writer.send(Bytes::from("ping")).await;
    }
    loop {
        if let Some(message) = rx.recv().await {
            let _ = writer.send(message).await;
        }
    }
}
