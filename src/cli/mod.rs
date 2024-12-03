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
