use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tsk_rs::parser::task::Task;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, value_name = "FILE", default_value = "tsk.toml")]
    config: PathBuf,

    /// Turn debugging information on
    #[clap(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// adds a new task from task description string
    New {
        /// task description string
        #[clap(allow_hyphen_values = true, multiple = true, value_parser)]
        descriptor: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::New { descriptor }) => {
            let task = Task::from_task_descriptor(&descriptor)?;
            println!("{:?}", task);
        },
        None => {}
    }

    Ok(())
}
