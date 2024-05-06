use clap::Parser;

#[derive(Parser, Debug)]
pub struct Cli {
    /// Config file to load
    #[arg(short, long, default_value = "config.toml")]
    pub config_file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_build_struct_for_parsing_cli_options() {
        let _ = env_logger::try_init();
        let options = vec!["node", "--config-file=conf.toml"];
        let args = Cli::try_parse_from(options).expect("Error parsing options");
        assert_eq!(args.config_file, "conf.toml");
    }

    #[test]
    fn it_should_get_default_values_for_args() {
        let options = vec!["node"];
        let args = Cli::try_parse_from(options).expect("Error parsing options");
        assert_eq!(args.config_file, "config.toml");
    }
}
