#![warn(missing_docs)]

//! Task management event monitor
//!
//! Command line utility for watching changes in tasks and notes

use std::{path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result};
use dotenv::dotenv;
use tsk_rs::{settings::{Settings, default_config, show_config}, notify::{FilesystemMonitor, DatabaseFileType, FileHandler}, task::{Task, load_task}, note::Note};

#[derive(Default)]
struct EventHandler;

impl FileHandler for EventHandler {
    fn handle(&self, file: DatabaseFileType, settings: Settings) {
        #[cfg(debug_assertions)]
        println!("file changed: {:?}", file);
        match file {
            DatabaseFileType::Task(_id) => {
                match Task::from_notify_event(file, &settings) {
                    Ok(task) => println!("[ Update for a task ] {}", task.description),
                    Err(error) => eprintln!("{:?}", error)
                };
            },
            DatabaseFileType::Note(_id) => {
                match Note::from_notify_event(file, &settings) {
                    Ok(note) => {
                        match load_task(&note.task_id.to_string(), &settings) {
                            Ok(task) => println!("[Update for a task note] {}", task.description),
                            Err(error) => eprintln!("{:?}", error)
                        };        
                    },
                    Err(error) => eprintln!("{:?}", error)
                };
            }
        };    
    }
}

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

    let handler = EventHandler::default();

    let mut monitor = FilesystemMonitor::new();
    monitor.watch(settings, Arc::new(handler), on_watch_error);

    Ok(())
}

fn on_watch_error(msg: String) {
    eprintln!("Error: {}", msg);
    std::process::exit(2);
}

// eof
