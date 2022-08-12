use std::path::PathBuf;
use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use cli_table::{Cell, Table, Style, format::Border, print_stdout};
use config::Config;
use hhmmss::Hhmmss;
use tsk_rs::{settings::{Settings}, task::Task};
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

#[derive(Subcommand)]
enum Commands {
    /// Show and/or list tasks
    Show {
        /// task id or part of one
        #[clap(value_parser)]
        id: Option<String>,
    },
    Start {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// optional annotation for the job at hand
        #[clap(value_parser)]
        annotation: Option<String>,
    },
    Stop {
        /// task id
        #[clap(value_parser)]
        id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = Config::builder()
        .add_source(config::File::with_name(cli.config.to_str().unwrap()))
        .add_source(config::Environment::with_prefix("TSK"))
        .build().with_context(|| {"while reading configuration"})?;
    let settings: Settings = config.try_deserialize().with_context(|| {"while applying defaults to configuration"})?;

    match &cli.command {
        Some(Commands::Show { id }) => {
            show_task(id, &settings)
        },
        Some(Commands::Start { id, annotation }) => {
            start_task(id, annotation, &settings)
        },
        Some(Commands::Stop { id }) => {
            stop_task(id, &settings)
        },
        None => {show_task(&None, &settings)}
    }
}

fn start_task(id: &String, annotation: &Option<String>, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.start(annotation).with_context(|| {"while starting time tracking"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
    println!("Started time tracking for task '{}'", task.id);
    Ok(())
}

fn stop_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.stop().with_context(|| {"while stopping time tracking"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
    println!("Stopped time tracking for task '{}'", task.id);
    Ok(())
}

fn show_task(id: &Option<String>, settings: &Settings) -> Result<()> {
    let mut task_cells = vec![];

    let mut task_pathbuf: PathBuf = settings.task_db_pathbuf().with_context(|| {"invalid data directory path configured"})?;
    if id.is_some() {
        task_pathbuf = task_pathbuf.join(format!("*{}*.yaml", id.as_ref().unwrap()));
    } else {
        task_pathbuf = task_pathbuf.join("*.yaml");
    }
    for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
        let task = Task::load_yaml_file_from(&task_filename?).with_context(|| {"while loading task from yaml file"})?;

        if task.is_running() {
            let runtime = task.runtime().unwrap();
            let runtime_str = Hhmmss::hhmmss(&runtime);
            task_cells.push(vec![task.id.cell(), task.description.cell(),
                task.project.unwrap_or_else(|| {"".to_string()}).cell(),
                runtime_str.cell(),
                ]);   
        }    
    }

    if !task_cells.is_empty() {
        let tasks_table = task_cells.table()
            .title(
                vec!["ID".cell().bold(true),
                "Description".cell().bold(true),
                "Project".cell().bold(true),
                "Cur. runtime".cell().bold(true)]) // headers of the table
            .border(Border::builder().build()); // empty border around the table
        print_stdout(tasks_table).with_context(|| {"while trying to print out pretty table of task(s)"})?;
    } else {
        println!("No tasks running");
    }

    Ok(())
}


// eof