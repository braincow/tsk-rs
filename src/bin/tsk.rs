use std::{path::PathBuf, fs::File, io::{Write, Read}};

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use config::Config;
use file_lock::{FileLock, FileOptions};
use tsk_rs::{task::Task, settings::Settings};
use glob::glob;

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
    /// List all tasks
    List,
    /// display the current configuration of the tsk-rs suite
    Config,
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
        },
        Some(Commands::List) => {
            list_tasks(&settings)
        },
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

fn list_tasks(settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.db_pathbuf()?.join("*.yaml");
    for task_filename in glob(task_pathbuf.to_str().unwrap())? {
        let task: Task;
        {
            let mut file = File::open(task_filename?).with_context(|| {"while opening task yaml file for reading"})?;
            let mut task_yaml: String = String::new();
            file.read_to_string(&mut task_yaml)?;
            task = Task::from_yaml_string(&task_yaml)?;
        }
        println!("{:?}", task);
    }
    
    Ok(())
}

// eof
