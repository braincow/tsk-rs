use std::{path::PathBuf, fs::remove_file};
use anyhow::{Result, Context};
use bat::{PrettyPrinter, Input};
use chrono::{Local, NaiveDateTime};
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, print_stdout, format::{Border, Separator}, Color};
use hhmmss::Hhmmss;
use question::{Answer, Question};
use tsk_rs::{task::{Task, TaskPriority}, settings::{Settings, show_config}};
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
    /// Adds a new task from task description string
    #[clap(allow_missing_positional = true)]
    New {
        /// ask description string
        #[clap(raw = true, value_parser)]
        descriptor: Vec<String>,
    },
    /// Show task definition and data
    Show {
        /// task id
        #[clap(value_parser)]
        id: String,
    },
    /// Show and/or list tasks
    List {
        /// task id or part of one
        #[clap(value_parser)]
        id: Option<String>,
        /// show also completed tasks
        #[clap(short, long, value_parser)]
        include_done: bool
    },
    /// Mark task as done
    Done {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// delete task file
        #[clap(short, long, value_parser)]
        delete: bool,
        /// delete file silently
        #[clap(short, long, value_parser)]
        force: bool,
    },
    /// Start tracking a task
    Start {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// optional annotation for the job at hand
        #[clap(value_parser)]
        annotation: Option<String>,
    },
    /// Stop from tracking a task
    Stop {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// complete task as well
        #[clap(value_parser)]
        complete: Option<bool>,
    },
    /// Edit raw datafile of the task (for advanced users)
    Edit {
        /// task id
        #[clap(value_parser)]
        id: String,
    },
    /// display the current configuration of the tsk-rs suite
    Config,
    /// Set task characteristics like priority, due date and etc
    Set {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// set priority to one in enum
        #[clap(short,long,value_enum)]
        priority: Option<TaskPriority>,
        /// set due date of the task
        #[clap(short,long,value_parser)]
        due_date: Option<NaiveDateTime>,
    },
    /// Unset task characteristics
    Unset {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// unset priority
        #[clap(short,long,value_parser)]
        priority: bool,
        /// unset due date
        #[clap(short,long,value_parser)]
        duedate: bool,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let settings = Settings::new(cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    match &cli.command {
        Some(Commands::Set { id, priority, due_date }) => {
            set_characteristic(id, priority, due_date, &settings)
        },
        Some(Commands::Unset { id, priority, duedate }) => {
            unset_characteristic(id, priority, duedate, &settings)
        },
        Some(Commands::New { descriptor }) => { 
            new_task(descriptor.join(" "), &settings)
        },
        Some(Commands::Show { id }) => {
            show_task(id, &settings)
        },
        Some(Commands::Config) => {
            show_config(&settings)
        },
        Some(Commands::List { id, include_done }) => {
            list_tasks(id, include_done, &settings)
        },
        Some(Commands::Done { id, delete, force}) => {
            complete_task(id, delete, force, &settings)
        },
        Some(Commands::Edit { id }) => {
            edit_task(id, &settings)
        },
        Some(Commands::Start { id, annotation }) => {
            start_task(id, annotation, &settings)
        },
        Some(Commands::Stop { id, complete }) => {
            stop_task(id, complete, &settings)
        },
        None => {list_tasks(&None, &false, &settings)}
    }
}

fn new_task(descriptor: String, settings: &Settings) -> Result<()> {
    let mut task = Task::from_task_descriptor(&descriptor).with_context(|| {"while parsing task descriptor"})?;
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", task.id)));
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
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

    let mut found_tasks: Vec<Task> = vec![];
    for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
        let task = Task::load_yaml_file_from(&task_filename?).with_context(|| {"while loading task from yaml file"})?;
        if !task.done || *include_done {
            found_tasks.push(task);
        }
    }
    found_tasks.sort_by_key(|k| k.score().unwrap());
    found_tasks.reverse();

    let mut task_cells = vec![];
    for found_task in found_tasks {
        let runtime_str = if found_task.is_running() {
            let runtime = found_task.current_runtime().unwrap();
            Hhmmss::hhmmss(&runtime)
        } else {
            "[stopped]".to_string()
        };
        let score = found_task.score()?;
        let cell_color: Option<Color> = if (7..12).contains(&score) {
            Some(Color::Green)
        } else if (13..18).contains(&score) {
            Some(Color::Yellow)
        } else if score > 19 {
            Some(Color::Red)
        } else {
            None
        };
        task_cells.push(vec![
            found_task.id.cell().foreground_color(cell_color),
            found_task.description.clone().cell().foreground_color(cell_color),
            found_task.project.clone().unwrap_or_else(|| {"".to_string()}).cell().foreground_color(cell_color),
            found_task.score()?.cell().foreground_color(cell_color),
            runtime_str.cell().foreground_color(cell_color),
            ]);
    }

    if !task_cells.is_empty() {
        let tasks_table = task_cells.table()
            .title(
                vec![
                    "Task ID".cell().bold(true).underline(true),
                    "Description".cell().bold(true).underline(true),
                    "Project".cell().bold(true).underline(true),
                    "Score".cell().bold(true).underline(true),
                    "Cur. runtime".cell().bold(true).underline(true),
                ]) // headers of the table
            .border(Border::builder().build())
            .separator(Separator::builder().build()); // empty border around the table
        print_stdout(tasks_table).with_context(|| {"while trying to print out pretty table of task(s)"})?;
    } else {
        println!("No tasks");
    }

    Ok(())
}

fn complete_task(id: &String, delete: &bool, force: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));

    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    if !delete {
        task.mark_as_completed().with_context(|| {"while modifying task"})?;
        task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving modified task yaml file"})?;
        println!("Task '{}' now marked as done.", task.id);
    } else {
        let answer = if !force {
            Question::new("Really delete this task?")
            .default(Answer::NO)
            .show_defaults()
            .confirm()
        } else {
            Answer::YES
        };

        if answer == Answer::YES {
            remove_file(task_pathbuf).with_context(|| {"while deleting task yaml file"})?;
            println!("Task '{}' now deleted permanently.", task.id);
        }
    }

    Ok(())
}

fn edit_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    let new_yaml = edit::edit_with_builder(task.to_yaml_string()?, edit::Builder::new().suffix(".yaml")).with_context(|| {"while starting an external editor"})?;
    task = Task::from_yaml_string(&new_yaml).with_context(|| {"while deserializing modified task yaml"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving modified task yaml file"})?;

    Ok(())
}

fn start_task(id: &String, annotation: &Option<String>, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.start(annotation).with_context(|| {"while starting time tracking"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
    println!("Started time tracking for task '{}'", task.id);
    Ok(())
}

fn stop_task(id: &String, complete: &Option<bool>, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.stop().with_context(|| {"while stopping time tracking"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
    println!("Stopped time tracking for task '{}'", task.id);
    
    if let Some(complete) = complete {
        if *complete {
            complete_task(id, &false, &false, settings)?;
        }
    }

    Ok(())
}

fn show_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file"})?;

    let task_yaml = task.to_yaml_string()?;
    PrettyPrinter::new()
        .language("yaml")
        .input(Input::from_bytes(task_yaml.as_bytes()))
        .colored_output(settings.output.colors)
        .grid(settings.output.grid)
        .print()
        .with_context(|| {"while trying to prettyprint yaml"})?;

    Ok(())
}

fn set_characteristic(id: &String, priority: &Option<TaskPriority>, due_date: &Option<NaiveDateTime>, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;

    let mut modified = false;

    if let Some(priority) = priority {
        let prio_str: &str = priority.into();
        task.metadata.insert("tsk-rs-task-priority".to_string(), prio_str.to_string());

        modified = true;
        println!("Priority was set/modified");
    }

    if let Some(due_date) = due_date {
        task.metadata.insert("tsk-rs-task-due-time".to_string(), due_date.and_local_timezone(Local).unwrap().to_rfc3339());
        modified = true;
        println!("Due date was set/modified");
    }

    if modified {
        task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
        println!("Modifications saved for task '{}'", task.id);
    }

    Ok(())
}

fn unset_characteristic(id: &String, priority: &bool, duedate: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;

    let mut modified = false;

    if *priority {
        let old_prio = task.metadata.remove("tsk-rs-task-priority");
        if old_prio.is_some() {
            modified = true;
            println!("Priority is now unset");
        }
    }

    if *duedate {
        let old_duedate = task.metadata.remove("tsk-rs-task-due-time");
        if old_duedate.is_some() {
            modified = true;
            println!("Due date is now unset");
        }
    }

    if modified {
        task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
        println!("Modifications saved for task '{}'", task.id);
    }

    Ok(())
}

// eof
