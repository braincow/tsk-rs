#![warn(missing_docs)]

//! Task management event monitor
//!
//! Command line utility for watching changes in tasks and notes

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result};
use dotenv::dotenv;
use tsk_rs::{settings::{Settings, default_config, show_config}, notify::{FilesystemMonitor, DatabaseFileType}, task::Task};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, env = "TSK_CONFIGFILE", value_name = "CONFIGFILE", default_value = default_config())]
    config: PathBuf,

    /// Sets the namespace of tasks
    #[clap(
        short,
        long,
        value_parser,
        env = "TSK_NAMESPACE",
        value_name = "NAMESPACE",
        default_value = "default"
    )]
    namespace: Option<String>,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Display the current configuration of the tsk-rs suite
    Config,
    /// Watch for the changes in database [default]
    Watch,
}

fn main() -> Result<()> {
    dotenv().ok();

    let cli = Cli::parse();

    let settings = Settings::new(cli.namespace, cli.config.to_str().unwrap())
        .with_context(|| "while loading settings")?;

    if settings.output.namespace {
        println!(" Namespace: '{}'", settings.namespace);
    }

    match &cli.command {
        Some(Commands::Watch) => watch(&settings),
        Some(Commands::Config) => show_config(&settings),
        None => watch(&settings)
    }
}

fn watch(settings: &Settings) -> Result<()> {
    // start monitoring the database folder for changes
    println!("Watching for task and note changes, CTRL+C to quit ...");

    let mut monitor = FilesystemMonitor::new();
    monitor.watch(settings, on_watch_change, on_watch_error);

    Ok(())
}

fn on_watch_error(msg: String) {
    eprintln!("Error: {}", msg);
    std::process::exit(2);
}

fn on_watch_change(file: DatabaseFileType) {
    #[cfg(debug_assertions)]
    println!("file changed: {:?}", file);
    match file {
        DatabaseFileType::Task(_id) => {
        },
        DatabaseFileType::Note(_id) => {
        }
    };
}

// eof