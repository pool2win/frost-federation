use crate::node::connection::Connection;
use tokio::net::{TcpListener, TcpStream};

mod connection;

#[derive(Debug)]
pub struct Node {
    pub seeds: Vec<String>,
    pub bind_address: String,
    //pub connections: Vec<Connection>,
    //sender: mpsc::Sender<[u8]>,
}

impl Node {
    /// Use builder pattern
    pub fn new() -> Self {
        Node {
            seeds: vec!["localhost:6680".to_string()],
            bind_address: "localhost".to_string(),
        }
    }

    pub fn seeds(self, seeds: Vec<String>) -> Self {
        let mut node = self;
        node.seeds = seeds;
        node
    }

    pub fn bind_address(self, address: String) -> Self {
        let mut node = self;
        node.bind_address = address;
        node
    }

    /// Start node by listening, accepting and connecting to peers
    pub async fn start(&mut self) {
        log::debug!("Starting...");
        self.connect_to_seeds().await;
        let listener = self.listen().await;
        self.start_accept(listener).await;
    }

    /// Start listening
    pub async fn listen(&mut self) -> TcpListener {
        log::debug!("Start listen...");
        TcpListener::bind(self.bind_address.to_string())
            .await
            .unwrap()
    }

    /// Start accepting connections
    pub async fn start_accept(&mut self, listener: TcpListener) {
        log::debug!("Start accepting...");
        loop {
            log::debug!("Waiting on accept...");
            let (stream, socket) = listener.accept().await.unwrap();
            log::info!("Accept connection from {}:{}", socket.ip(), socket.port());
            tokio::spawn(async move {
                let connection = Connection::new(stream);
                connection.start(false).await;
            });
        }
    }

    /// Connect to all peers and start reader writer tasks
    pub async fn connect_to_seeds(&mut self) {
        log::debug!("Connecting to seeds...");
        for seed in self.seeds.iter() {
            log::debug!("Connecting to seed {}", seed);
            if let Ok(stream) = TcpStream::connect(seed).await {
                tokio::spawn(async move {
                    let connection = Connection::new(stream);
                    connection.start(true).await;
                });
            } else {
                log::debug!("Failed to connect to seed {}", seed);
            }
        }
    }
}