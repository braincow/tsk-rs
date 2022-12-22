use std::{path::PathBuf, fs::remove_file};
use anyhow::{Result, Context};
use bat::{PrettyPrinter, Input};
use chrono::NaiveDateTime;
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, print_stdout, format::{Border, Separator}, Color};
use hhmmss::Hhmmss;
use question::{Answer, Question};
use tsk_rs::{task::{Task, TaskPriority, new_task, start_task, load_task, save_task, stop_task, task_pathbuf_from_task, list_tasks, amount_of_tasks}, settings::{Settings, show_config, default_config}, metadata::MetadataKeyValuePair};
use dotenv::dotenv;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, env = "TSK_CONFIGFILE", value_name = "CONFIGFILE", default_value = default_config())]
    config: PathBuf,

    /// Sets the namespace of tasks
    #[clap(short, long, value_parser, env = "TSK_NAMESPACE", value_name = "NAMESPACE", default_value = "default")]
    namespace: Option<String>,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds a new task from task description string
    #[clap(trailing_var_arg = true)]
    New {
        /// Task description string
        #[clap(value_parser)]
        descriptor: Vec<String>,
    },
    /// Show task definition and data
    Show {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
    },
    /// List task(s)
    List {
        /// Search a word or a part of word from description, project and/or tags. Empty will list all.
        #[clap(value_parser)]
        search: Option<String>,
        /// Include also completed tasks
        #[clap(short, long, value_parser)]
        include_done: bool
    },
    /// Mark task as done and stop time tracking if running
    Done {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
    },
    /// Delete task file permanently
    Delete {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
        /// Delete file silently
        #[clap(short, long, value_parser)]
        force: bool,
    },
    /// Start tracking a task
    #[clap(trailing_var_arg = true)]
    Start {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
        /// Optional annotation for the job at hand
        #[clap(value_parser)]
        annotation: Vec<String>,
    },
    /// Stop from tracking a task
    Stop {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
        /// Also, mark task as done immediately
        #[clap(short, long, value_parser)]
        done: bool,
    },
    /// Edit raw datafile of the task (for advanced users)
    Edit {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
    },
    /// Display the current configuration of the tsk-rs suite
    Config,
    /// Set task characteristics like priority, due date and etc
    Set {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
        /// Set/change priority of the task
        #[clap(long,value_enum)]
        priority: Option<TaskPriority>,
        /// Set/change due date of the task (YYYY-MM-DDTHH:MM:SS)
        #[clap(long,value_parser)]
        due_date: Option<NaiveDateTime>,
        /// Add tag to task
        #[clap(long,value_parser)]
        tag: Option<Vec<String>>,
        /// Set/change project of the task
        #[clap(long, value_parser)]
        project: Option<String>,
        /// Add/change metadata of the task: x-key=value
        #[clap(long,value_parser)]
        metadata: Option<Vec<MetadataKeyValuePair>>,
    },
    /// Unset task characteristics
    Unset {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
        /// Unset priority
        #[clap(long,value_parser)]
        priority: bool,
        /// Unset due date
        #[clap(long,value_parser)]
        due_date: bool,
        /// Remove tag(s) from task.
        #[clap(long,value_parser)]
        tag: Option<Vec<String>>,
        /// Remove project from task
        #[clap(long,value_parser)]
        project: bool,
        /// Remove metadata(s) from task
        #[clap(long,value_parser)]
        metadata: Option<Vec<String>>,
    },
    /// Shorthand: set 'hold' special tag for task
    Hold {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
    },
    /// Shorthand: set 'next' special tag for task
    Next {
        /// Existing task id
        #[clap(value_parser)]
        id: String,
    },
}

fn main() -> Result<()> {
    dotenv().ok();

    let cli = Cli::parse();

    let settings = Settings::new(cli.namespace, cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    if settings.output.namespace {
        println!(" Namespace: '{}'", settings.namespace);
    }

    match &cli.command {
        Some(Commands::Set { id, priority, due_date,
                tag, project, metadata }) => {
            cli_set_characteristic(id, priority, due_date, tag, project, metadata, &settings)
        },
        Some(Commands::Unset { id, priority, due_date,
                tag, project, metadata }) => {
            cli_unset_characteristic(id, priority, due_date, tag, project, metadata, &settings)
        },
        Some(Commands::New { descriptor }) => { 
            cli_new_task(descriptor.join(" "), &settings)
        },
        Some(Commands::Show { id }) => {
            show_task(id, &settings)
        },
        Some(Commands::Config) => {
            show_config(&settings)
        },
        Some(Commands::List { search, include_done }) => {
            cli_list_tasks(search, include_done, &settings)
        },
        Some(Commands::Done { id }) => {
            cli_complete_task(id, &settings)
        },
        Some(Commands::Delete { id, force }) => {
            delete_task(id, force, &settings)
        }
        Some(Commands::Edit { id }) => {
            edit_task(id, &settings)
        },
        Some(Commands::Start { id, annotation }) => {
            if !annotation.is_empty() {
                cli_start_task(id, &Some(annotation.join(" ")), &settings)
            } else {
                cli_start_task(id, &None, &settings)
            }
        },
        Some(Commands::Stop { id, done }) => {
            cli_stop_task(id, done, &settings)
        },
        Some(Commands::Hold { id}) => cli_set_characteristic(id, &None, &None, &Some(vec!["hold".to_string()]), &None, &None, &settings),
        Some(Commands::Next { id}) => cli_set_characteristic(id, &None, &None, &Some(vec!["next".to_string()]), &None, &None, &settings),
        None => {cli_list_tasks(&None, &false, &settings)}
    }
}

fn cli_new_task(descriptor: String, settings: &Settings) -> Result<()> {
    let task = new_task(descriptor, settings)?;
    println!("Created a task '{}'", task.id);
    Ok(())
}

fn cli_list_tasks(search: &Option<String>, include_done: &bool, settings: &Settings) -> Result<()> {
    let found_tasks = list_tasks(search, include_done, settings)?;
    let total_tasks_count: usize = amount_of_tasks(settings, false)?;

    let mut task_cells = vec![];
    let mut found_tasks_count: usize = 0;
    for found_task in found_tasks {
        found_tasks_count += 1;

        let runtime_str = if found_task.is_running() {
            let runtime = found_task.current_runtime().unwrap();
            Hhmmss::hhmmss(&runtime)
        } else {
            "[stopped]".to_string()
        };
        let mut cell_color: Option<Color> = None;
        if settings.output.colors {
            cell_color = match found_task.score()? {
                7..=12 => Some(Color::Green),
                13..=18 => Some(Color::Yellow),
                n if n >= 19 => Some(Color::Red),
                _ => None
            };    
        }

        let mut desc = found_task.description.clone();
        if desc.len() > settings.output.descriptionlength + 3 {
            // if the desc truncated to max length plus three dot characters is
            //  shorter than the max len then truncate it and add those three dots
            desc = format!("{}...", &desc[..settings.output.descriptionlength]);
        }

        let description = if let Some(tags) = found_task.tags.clone() {
            if settings.task.specialvisible {
                // make special tags visible
                if tags.contains(&"next".to_string()) {
                    desc = format!("{} #next", desc);
                }
                if tags.contains(&"hold".to_string()) {
                    desc = format!("{} #hold", desc);
                }
                if tags.contains(&"start".to_string()) {
                    desc = format!("{} #start", desc);
                }
            }
            desc
        } else {
            desc
        };

        task_cells.push(vec![
            found_task.id.cell().foreground_color(cell_color),
            description.cell().foreground_color(cell_color),
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
        if settings.output.totals {
            println!("\n Number of tasks: {}/{}", found_tasks_count, total_tasks_count);
        }
    } else {
        println!("No tasks");
    }

    Ok(())
}

fn cli_complete_task(id: &String, settings: &Settings) -> Result<()> {
    let mut task = load_task(id, settings)?;
    task.mark_as_completed().with_context(|| {"while marking task as completed"})?;
    save_task(&mut task, settings).with_context(|| {"while saving modified task yaml file"})?;
    println!("Task '{}' now marked as done.", task.id);

    Ok(())
}

fn delete_task(id: &String, force: &bool, settings: &Settings) -> Result<()> {
    let task = load_task(id, settings)?;

    let answer = if !force {
        Question::new("Really delete this task?")
        .default(Answer::NO)
        .show_defaults()
        .confirm()
    } else {
        Answer::YES
    };

    if answer == Answer::YES {
        remove_file(task_pathbuf_from_task(&task, settings)?).with_context(|| {"while deleting task yaml file"})?;
        println!("Task '{}' now deleted permanently.", task.id);
    }

    Ok(())
}

fn edit_task(id: &String, settings: &Settings) -> Result<()> {
    let mut task = load_task(id, settings)?;

    let mut modified = false;

    let new_yaml = edit::edit_with_builder(task.to_yaml_string()?, edit::Builder::new().suffix(".yaml")).with_context(|| {"while starting an external editor"})?;

    if new_yaml != task.to_yaml_string()? {
        task = Task::from_yaml_string(&new_yaml).with_context(|| {"while deserializing modified task yaml"})?;
        modified = true;
    }

    if modified {
        save_task(&mut task, settings).with_context(|| {"while saving modified task yaml file"})?;
        println!("Task '{}' was updated.", task.id);
    }

    Ok(())
}

fn cli_start_task(id: &String, annotation: &Option<String>, settings: &Settings) -> Result<()> {
    let task = start_task(id, annotation, settings)?;
    println!("Started time tracking for task '{}'", task.id);

    Ok(())
}

fn cli_stop_task(id: &String, done: &bool, settings: &Settings) -> Result<()> {
    let task = stop_task(id, done, settings)?;
    println!("Stopped time tracking for task '{}'", task.id);

    Ok(())
}

fn show_task(id: &String, settings: &Settings) -> Result<()> {
    let task = load_task(id, settings)?;
    let task_yaml = task.to_yaml_string()?;

    PrettyPrinter::new()
        .language("yaml")
        .input(Input::from_bytes(task_yaml.as_bytes()))
        .colored_output(settings.output.colors)
        .grid(settings.output.grid)
        .line_numbers(settings.output.numbers)
        .print()
        .with_context(|| {"while trying to prettyprint yaml"})?;

    Ok(())
}

fn cli_set_characteristic(id: &String, priority: &Option<TaskPriority>, due_date: &Option<NaiveDateTime>,
    tags: &Option<Vec<String>>, project: &Option<String>, metadata: &Option<Vec<MetadataKeyValuePair>>, settings: &Settings) -> Result<()> {
    let mut task = load_task(id, settings)?;
    let modified = task.set_characteristic(priority, due_date, tags, project, metadata);
    
    if modified {
        save_task(&mut task, settings)?;
        println!("Task characteristics modified for '{}'", task.id);
    }
    
    Ok(())
}

fn cli_unset_characteristic(id: &String, priority: &bool, due_date: &bool,
    tags: &Option<Vec<String>>, project: &bool, metadata: &Option<Vec<String>>, settings: &Settings) -> Result<()> {
    let mut task = load_task(id, settings)?;
    let modified = task.unset_characteristic(priority, due_date, tags, project, metadata);

    if modified {
        save_task(&mut task, settings)?;
        println!("Task characteristics modified for '{}'", task.id);
    }

    Ok(())
}

// eof
