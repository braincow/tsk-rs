use std::{path::PathBuf, fs::{create_dir_all}, io::Write};

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use file_lock::{FileLock, FileOptions};
use serde::{Serialize, Deserialize};
use tsk_rs::task::Task;
use directories::ProjectDirs;

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

// https://github.com/clap-rs/clap/issues/1236
#[derive(Subcommand)]
enum Commands {
    /// adds a new task from task description string
    #[clap(allow_missing_positional = true)]
    New {
        /// task description string
        #[clap(raw = true, value_parser)]
        descriptor: Vec<String>,
    },
    /// display the current configuration of the tsk-rs suite
    Config,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub db_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        let proj_dirs = ProjectDirs::from("", "",  "tsk-rs").unwrap();

        Self {
            db_path: String::from(proj_dirs.data_dir().to_str().unwrap())
        }
    }
}

impl Settings {
    fn db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = PathBuf::from(&self.db_path);
        if !pathbuf.is_dir() {
            create_dir_all(&pathbuf)?;
        }
        Ok(pathbuf)
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = Config::builder()
        .add_source(config::File::with_name(cli.config.to_str().unwrap()))
        .add_source(config::Environment::with_prefix("TSK"))
        .build()?;
    let settings: Settings = config.try_deserialize()?;

    match &cli.command {
        Some(Commands::New { descriptor }) => { 
            new_task(descriptor.join(" "), &settings)
        },
        Some(Commands::Config) => {
            println!("{:?}", settings);
            Ok(())
        }
        None => {panic!("unknown cli command");}
    }
}

fn new_task(descriptor: String, settings: &Settings) -> Result<()> {
    let task = Task::from_task_descriptor(&descriptor)?;
    let task_pathbuf = settings.db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", task.id)));

    let should_we_block  = true;
    let options = FileOptions::new()
        .write(true)
        .create(true)
        .append(true);
    {
        let mut filelock= FileLock::lock(task_pathbuf, should_we_block, options)?;
        filelock.file.write_all(task.to_yaml_string()?.as_bytes())?;
        filelock.file.flush()?;
        filelock.file.sync_all()?;    
    }
    println!("Created a task '{}'", task.id);
    Ok(())
}
