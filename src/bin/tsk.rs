use std::{path::PathBuf, fs::remove_file};

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, print_stdout};
use config::Config;
use tsk_rs::{task::Task, settings::Settings};
use glob::glob;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, value_name = "FILE", default_value = "tsk.toml")]
    config: PathBuf,

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
    /// Show and/or list tasks
    Show {
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
    Start {
        /// task id
        #[clap(value_parser)]
        id: String,
    },
    Stop {
        /// task id
        #[clap(value_parser)]
        id: String,
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
            println!("{}", settings);
            Ok(())
        },
        Some(Commands::Show { id, include_done }) => {
            show_tasks(id, include_done, &settings)
        },
        Some(Commands::Done { id, delete}) => {
            complete_task(id, delete, &settings)
        },
        Some(Commands::Start { id }) => {
            start_task(id, &settings)
        },
        Some(Commands::Stop { id }) => {
            stop_task(id, &settings)
        },
        None => {show_tasks(&None, &false, &settings)}
    }
}

fn start_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.start();
    task.save_yaml_file_to(&task_pathbuf).with_context(|| {"while saving task yaml file"})?;
    println!("Started time tracking for task '{}'", task.id);
    Ok(())
}

fn stop_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.stop();
    task.save_yaml_file_to(&task_pathbuf).with_context(|| {"while saving task yaml file"})?;
    println!("Stopped time tracking for task '{}'", task.id);
    Ok(())
}

fn new_task(descriptor: String, settings: &Settings) -> Result<()> {
    let mut task = Task::from_task_descriptor(&descriptor).with_context(|| {"while parsing task descriptor"})?;
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", task.id)));
    task.save_yaml_file_to(&task_pathbuf).with_context(|| {"while saving task yaml file"})?;
    println!("Created a task '{}'", task.id);
    Ok(())
}

fn show_tasks(id: &Option<String>, include_done: &bool, settings: &Settings) -> Result<()> {
    let mut task_cells = vec![];

    let mut task_pathbuf: PathBuf = settings.task_db_pathbuf().with_context(|| {"invalid data directory path configured"})?;
    if id.is_some() {
        task_pathbuf = task_pathbuf.join(format!("*{}*.yaml", id.as_ref().unwrap()));
    } else {
        task_pathbuf = task_pathbuf.join("*.yaml");
    }
    for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
        let task = Task::load_yaml_file_from(&task_filename?).with_context(|| {"while loading task from yaml file"})?;
        let runtime = task.runtime();
        let runtime_str = if let Some(runtime) = runtime {
            runtime.to_string()
        } else {
            "".to_string()
        };
        if !task.is_done() || *include_done {
            task_cells.push(vec![task.id.cell(), task.description.cell(),
                task.project.unwrap_or_else(|| {"".to_string()}).cell(),
                runtime_str.cell(),
                ]);
        }
    }
    if !task_cells.is_empty() {
        let tasks_table = task_cells.table().title(vec!["ID".cell().bold(true), "Description".cell().bold(true), "Project".cell().bold(true), "Cur. runtime".cell().bold(true)]);
        print_stdout(tasks_table).with_context(|| {"while trying to print out pretty listing of task(s)"})?;
    } else {
        println!("No tasks");
    }

    Ok(())
}

fn complete_task(id: &String, delete: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));

    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    if !delete {
        task.mark_as_completed();
        task.save_yaml_file_to(&task_pathbuf).with_context(|| {"while saving modified task yaml file"})?;
        println!("Task '{}' now marked as done.", task.id);
    } else {
        remove_file(task_pathbuf).with_context(|| {"while deleting task yaml file"})?;
        println!("Task '{}' now deleted permanently.", task.id);
    }

    Ok(())
}

// eof
