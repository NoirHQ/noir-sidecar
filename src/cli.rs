use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    /// The config file to use
    #[arg(short, long, default_value = "configs/config.toml")]
    pub config: PathBuf,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}
