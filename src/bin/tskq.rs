use std::path::PathBuf;
use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tsk_rs::settings::{Settings, show_config};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, value_name = "FILE", default_value = "tsk.toml")]
    config: PathBuf,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// display the current configuration of the tsk-rs suite
    Config,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let settings = Settings::new(cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    match &cli.command {
        Some(Commands::Config) => {
            show_config(&settings)
        },
        None => {todo!()}
    }
}

// eof