use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;

/// Struct to capture configuration from config.toml
#[derive(Deserialize, Default, Debug, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub network: NetworkConfig,

    #[serde(default)]
    pub peer: PeerConfig,
}

/// Struct to capture network section of configuration from config.toml
#[derive(Deserialize, Debug, PartialEq)]
pub struct NetworkConfig {
    #[serde(default = "default_bind")]
    pub bind: String,

    #[serde(default = "default_port")]
    pub port: usize,
}

fn default_bind() -> String {
    NetworkConfig::default().bind
}

fn default_port() -> usize {
    NetworkConfig::default().port
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            bind: "localhost".to_string(),
            port: 6680,
        }
    }
}

/// Struct to capture peer section of configuration from config.toml
#[derive(Deserialize, Debug, PartialEq)]
pub struct PeerConfig {
    #[serde(default = "default_seeds")]
    pub seeds: Vec<String>,

    #[serde(default = "default_max_peer_count")]
    pub max_peer_count: usize,

    #[serde(default = "default_max_pending_messages")]
    pub max_pending_messages: usize,

    #[serde(default = "default_max_pending_send_to_all")]
    pub max_pending_send_to_all: usize,

    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
}

fn default_seeds() -> Vec<String> {
    PeerConfig::default().seeds
}

fn default_max_peer_count() -> usize {
    PeerConfig::default().max_peer_count
}

fn default_max_pending_messages() -> usize {
    PeerConfig::default().max_pending_messages
}

fn default_max_pending_send_to_all() -> usize {
    PeerConfig::default().max_pending_send_to_all
}

fn default_heartbeat_interval() -> u64 {
    PeerConfig::default().heartbeat_interval
}

impl Default for PeerConfig {
    fn default() -> Self {
        PeerConfig {
            seeds: Vec::<String>::new(),
            max_peer_count: 10,
            max_pending_messages: 32,
            max_pending_send_to_all: 128,
            heartbeat_interval: 1000,
        }
    }
}

/// Load configuraton from config.toml file.
/// If no file we provided, we use the default configuration
pub fn load_config_from_file(path: String) -> Option<Config> {
    match read_file(path) {
        Err(_) => {
            log::info!("No config.toml file provided, using defaults.");
            Some(Config::default())
        }
        Ok(contents) => parse_config_from_string(contents),
    }
}

/// Parse config.toml file contents into Conf struct.
fn parse_config_from_string(contents: String) -> Option<Config> {
    let config = toml::from_str(&contents);
    match config {
        Ok(config) => config,
        Err(e) => {
            log::info!("Error parsing config file {:?}", e);
            None
        }
    }
}

/// Read file path provided into a string.
fn read_file(path: String) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Build the bind address from network config
pub fn get_bind_address(network_config: NetworkConfig) -> String {
    let mut bind_address = network_config.bind;
    bind_address.push(':');
    bind_address.push_str(network_config.port.to_string().as_str());
    bind_address
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_load_default_config_correctly() {
        let conf = Config::default();
        let network = conf.network;
        let peer = conf.peer;
        assert_eq!(network.bind, "localhost".to_string());
        assert_eq!(network.port, 6680);
        assert_eq!(peer.seeds.len(), 0);
        assert_eq!(peer.max_peer_count, 10);
        assert_eq!(peer.max_pending_messages, 32);
        assert_eq!(peer.max_pending_send_to_all, 128);
        assert_eq!(peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_load_empty_config_for_empty_string() {
        let conf = parse_config_from_string(r#""#.to_string()).unwrap();
        assert_eq!(conf.network.bind, "localhost".to_string());
        assert_eq!(conf.network.port, 6680);
        assert_eq!(conf.peer.seeds.len(), 0);
        assert_eq!(conf.peer.max_peer_count, 10);
        assert_eq!(conf.peer.max_pending_messages, 32);
        assert_eq!(conf.peer.max_pending_send_to_all, 128);
        assert_eq!(conf.peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_return_error_for_bad_toml() {
        assert!(parse_config_from_string(r#"abcd"#.to_string()).is_none());
    }

    #[test]
    fn it_should_load_default_for_missing_fields() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(
            r#"
    [network]
    bind="localhost""#
                .to_string(),
        )
        .unwrap();
        assert_eq!(conf.network.bind, "localhost".to_string());
        assert_eq!(conf.network.port, 6680);
        assert_eq!(conf.peer.seeds.len(), 0);
        assert_eq!(conf.peer.max_peer_count, 10);
        assert_eq!(conf.peer.max_pending_messages, 32);
        assert_eq!(conf.peer.max_pending_send_to_all, 128);
        assert_eq!(conf.peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_load_default_for_missing_fields_for_network() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(
            r#"
    [network]"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(conf.network.bind, "localhost".to_string());
        assert_eq!(conf.network.port, 6680);
        assert_eq!(conf.peer.seeds.len(), 0);
        assert_eq!(conf.peer.max_peer_count, 10);
        assert_eq!(conf.peer.max_pending_messages, 32);
        assert_eq!(conf.peer.max_pending_send_to_all, 128);
        assert_eq!(conf.peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_load_default_for_missing_fields_for_peer() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(
            r#"
    [peer]"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(conf.network.bind, "localhost".to_string());
        assert_eq!(conf.network.port, 6680);
        assert_eq!(conf.peer.seeds.len(), 0);
        assert_eq!(conf.peer.max_peer_count, 10);
        assert_eq!(conf.peer.max_pending_messages, 32);
        assert_eq!(conf.peer.max_pending_send_to_all, 128);
        assert_eq!(conf.peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_load_default_for_missing_fields_in_multiple_sections() {
        let _ = env_logger::try_init();
        let conf = parse_config_from_string(
            r#"
    [network]
    bind="localhost"
    [peer]
    max_peer_count = 100
    seeds = ["1.2.3.4:8080"]"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(conf.network.bind, "localhost".to_string());
        let peer = conf.peer;
        assert_eq!(peer.seeds, vec!["1.2.3.4:8080".to_string()]);
        assert_eq!(peer.max_peer_count, 100);
        assert_eq!(peer.max_pending_messages, 32);
        assert_eq!(peer.max_pending_send_to_all, 128);
        assert_eq!(peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_load_config_without_errors() {
        let _ = env_logger::try_init();
        let conf = load_config_from_file("config.toml".to_string()).unwrap();
        let network = conf.network;
        let peer = conf.peer;
        assert_eq!(network.bind, "localhost");
        assert_eq!(network.port, 6680);
        assert_eq!(peer.seeds.len(), 0);
        assert_eq!(peer.max_peer_count, 10);
        assert_eq!(peer.max_pending_messages, 32);
        assert_eq!(peer.max_pending_send_to_all, 128);
        assert_eq!(peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_load_default_config_when_file_not_found() {
        let _ = env_logger::try_init();
        let conf = load_config_from_file("no_such_file.toml".to_string()).unwrap();
        let network = conf.network;
        let peer = conf.peer;

        assert_eq!(network.bind, "localhost");
        assert_eq!(network.port, 6680);
        assert_eq!(peer.seeds.len(), 0);
        assert_eq!(peer.max_peer_count, 10);
        assert_eq!(peer.max_pending_messages, 32);
        assert_eq!(peer.max_pending_send_to_all, 128);
        assert_eq!(peer.heartbeat_interval, 1000);
    }

    #[test]
    fn it_should_get_bind_address_from_config() {
        let _ = env_logger::try_init();
        let conf = load_config_from_file("config.toml".to_string()).unwrap();
        let network = conf.network;
        assert_eq!(get_bind_address(network), "localhost:6680");
    }
}
