use std::{path::PathBuf, fs::{remove_file, File}, io::{Write, Read}};

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
    /// List and show all tasks
    List {
        /// task id or part of one
        #[clap(value_parser)]
        id: Option<String>,
        /// show also completed tasks
        #[clap(short, long, value_parser)]
        include_done: bool
    },
    Done {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// delete task file
        #[clap(short, long, value_parser)]
        delete: bool
    },
    /// display the current configuration of the tsk-rs suite
    Config,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = Config::builder()
        .add_source(config::File::with_name(cli.config.to_str().unwrap()))
        .add_source(config::Environment::with_prefix("TSK"))
        .build().with_context(|| {"while reading configuration"})?;
    let settings: Settings = config.try_deserialize().with_context(|| {"while applying defaults to configuration"})?;

    match &cli.command {
        Some(Commands::New { descriptor }) => { 
            new_task(descriptor.join(" "), &settings)
        },
        Some(Commands::Config) => {
            println!("{:?}", settings);
            Ok(())
        },
        Some(Commands::List { id, include_done }) => {
            list_tasks(id, include_done, &settings)
        },
        Some(Commands::Done { id, delete}) => {
            complete_task(id, delete, &settings)
        }
        None => {panic!("unknown cli command");}
    }
}

fn new_task(descriptor: String, settings: &Settings) -> Result<()> {
    let task = Task::from_task_descriptor(&descriptor).with_context(|| {"while parsing task descriptor"})?;
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", task.id)));

    let should_we_block  = true;
    let options = FileOptions::new()
        .write(true)
        .create(true)
        .append(true);
    {
        let mut filelock= FileLock::lock(task_pathbuf, should_we_block, options)
            .with_context(|| {"while opening new task yaml file"})?;
        filelock.file.write_all(task.to_yaml_string().with_context(|| {"while serializing task struct to yaml"})?.as_bytes()).with_context(|| {"while writing to task yaml file"})?;
        filelock.file.flush().with_context(|| {"while flushing os caches to disk"})?;
        filelock.file.sync_all().with_context(|| {"while syncing filesystem metadata"})?;
    }
    println!("Created a task '{}'", task.id);
    Ok(())
}

fn list_tasks(id: &Option<String>, include_done: &bool, settings: &Settings) -> Result<()> {
    let mut task_pathbuf: PathBuf = settings.task_db_pathbuf().with_context(|| {"invalid data directory path configured"})?;
    if id.is_some() {
        task_pathbuf = task_pathbuf.join(format!("*{}*.yaml", id.as_ref().unwrap()));
    } else {
        task_pathbuf = task_pathbuf.join("*.yaml");
    }
    for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
        let task: Task;
        {
            let mut file = File::open(task_filename?).with_context(|| {"while opening task yaml file for reading"})?;
            let mut task_yaml: String = String::new();
            file.read_to_string(&mut task_yaml).with_context(|| {"while reading task yaml file"})?;
            task = Task::from_yaml_string(&task_yaml).with_context(|| {"while serializing yaml into task struct"})?;
        }
        if !task.is_done() || *include_done {
            println!("{:?}", task);
        }
    }

    Ok(())
}

fn complete_task(id: &String, delete: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));

    if !delete {
        let mut task: Task;

        let should_we_block  = true;
        let options = FileOptions::new()
            .write(true)
            .create(false)
            .append(false);
        {
            let mut filelock= FileLock::lock(task_pathbuf, should_we_block, options).with_context(|| {"while opening task yaml file for editing"})?;
            let mut task_yaml: String = String::new();
            filelock.file.read_to_string(&mut task_yaml).with_context(|| {"while reading task yaml file"})?;
            task = Task::from_yaml_string(&task_yaml).with_context(|| {"while serializing yaml into task struct"})?;
            task.mark_as_completed();
            filelock.file.write_all(task.to_yaml_string().with_context(|| {"while serializing task struct to yaml"})?.as_bytes()).with_context(|| {"while writing to task yaml file"})?;
            filelock.file.flush().with_context(|| {"while flushing os caches to disk"})?;
            filelock.file.sync_all().with_context(|| {"while syncing filesystem metadata"})?;
            }
    } else {
        remove_file(task_pathbuf).with_context(|| {"while deleting task yaml file"})?;
    }

    Ok(())
}

// eof
