use std::{path::PathBuf, fs::remove_file};
use anyhow::{Result, Context};
use bat::{PrettyPrinter, Input};
use chrono::{Local, NaiveDateTime};
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, print_stdout, format::{Border, Separator}, Color};
use hhmmss::Hhmmss;
use question::{Answer, Question};
use tsk_rs::{task::{Task, TaskPriority}, settings::{Settings, show_config, default_config}, metadata::MetadataKeyValuePair};
use glob::glob;
use dotenv::dotenv;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, env = "TSK_CONFIGFILE", value_name = "CONFIGFILE", default_value = default_config())]
    config: PathBuf,

    /// Sets the namespace of tasks
    #[clap(short, long, value_parser, value_name = "NAMESPACE")]
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
            set_characteristic(id, priority, due_date, tag, project, metadata, &settings)
        },
        Some(Commands::Unset { id, priority, due_date,
                tag, project, metadata }) => {
            unset_characteristic(id, priority, due_date, tag, project, metadata, &settings)
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
        Some(Commands::List { search, include_done }) => {
            list_tasks(search, include_done, &settings)
        },
        Some(Commands::Done { id }) => {
            complete_task(id, &settings)
        },
        Some(Commands::Delete { id, force }) => {
            delete_task(id, force, &settings)
        }
        Some(Commands::Edit { id }) => {
            edit_task(id, &settings)
        },
        Some(Commands::Start { id, annotation }) => {
            if !annotation.is_empty() {
                start_task(id, &Some(annotation.join(" ")), &settings)
            } else {
                start_task(id, &None, &settings)
            }
        },
        Some(Commands::Stop { id, done }) => {
            stop_task(id, done, &settings)
        },
        Some(Commands::Hold { id}) => set_characteristic(id, &None, &None, &Some(vec!["hold".to_string()]), &None, &None, &settings),
        Some(Commands::Next { id}) => set_characteristic(id, &None, &None, &Some(vec!["next".to_string()]), &None, &None, &settings),
        None => {list_tasks(&None, &false, &settings)}
    }
}

fn new_task(descriptor: String, settings: &Settings) -> Result<()> {
    let mut task = Task::from_task_descriptor(&descriptor).with_context(|| {"while parsing task descriptor"})?;
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", task.id)));
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;

    // once the task file has been created check for special tags that should take immediate action
    if let Some(tags) = task.tags.clone() {
        if tags.contains(&"start".to_string()) && settings.task.starttag {
            start_task(&task.id.to_string(), &Some("started on creation".to_string()), settings)?;
        }
    }

    println!("Created a task '{}'", task.id);
    Ok(())
}

fn list_tasks(search: &Option<String>, include_done: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf: PathBuf = settings.task_db_pathbuf().with_context(|| {"invalid data directory path configured"})?.join("*.yaml");

    let mut found_tasks: Vec<Task> = vec![];
    let mut total_tasks_count: usize = 0;
    for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
        // if the filename is u-u-i-d.3.yaml for example it is a backup file and should be disregarded
        if task_filename.as_ref().unwrap().file_name().unwrap().to_string_lossy().split('.').collect::<Vec<_>>()[1] != "yaml" {
            continue;
        }
        total_tasks_count += 1;

        let task = Task::load_yaml_file_from(&task_filename?).with_context(|| {"while loading task from yaml file"})?;

        if !task.done || *include_done {
            if let Some(search) = search {
                if task.loose_match(search) {
                    // a part of key information matches search term, so the task is included
                    found_tasks.push(task);
                }
            } else {
                // search term is empty so everything matches
                found_tasks.push(task);
            }
        }
    }
    found_tasks.sort_by_key(|k| k.score().unwrap());
    found_tasks.reverse();

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

fn complete_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;

    if task.is_running() && settings.task.stopondone {
        // task is running, so first stop it
        stop_task(id, &false, settings)?;
    }

    // remove special tags when task is marked completed
    if settings.task.clearpsecialtags {
        unset_characteristic(id, &false, &false, 
            &Some(vec!["start".to_string(), "next".to_string(), "hold".to_string()]),
            &false, &None, settings)?;    
    }

    task.mark_as_completed().with_context(|| {"while modifying task"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving modified task yaml file"})?;
    println!("Task '{}' now marked as done.", task.id);

    Ok(())
}

fn delete_task(id: &String, force: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;

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

    Ok(())
}

fn edit_task(id: &String, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;

    let mut modified = false;

    let new_yaml = edit::edit_with_builder(task.to_yaml_string()?, edit::Builder::new().suffix(".yaml")).with_context(|| {"while starting an external editor"})?;

    if new_yaml != task.to_yaml_string()? {
        task = Task::from_yaml_string(&new_yaml).with_context(|| {"while deserializing modified task yaml"})?;
        modified = true;
    }

    if modified {
        task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving modified task yaml file"})?;
        println!("Task '{}' was updated.", task.id);
    } else {
        println!("No updates made to task '{}'.", task.id);
    }

    Ok(())
}

fn start_task(id: &String, annotation: &Option<String>, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.start(annotation).with_context(|| {"while starting time tracking"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;

    // if special tag (hold) is present then release the hold by modifying tags.
    if settings.task.autorelease {
        unset_characteristic(id, &false, &false, &Some(vec!["hold".to_string()]),
            &false, &None, settings)?;
    }

    println!("Started time tracking for task '{}'", task.id);
    Ok(())
}

fn stop_task(id: &String, done: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for editing"})?;
    task.stop().with_context(|| {"while stopping time tracking"})?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
    println!("Stopped time tracking for task '{}'", task.id);
    
    if *done {
        complete_task(id, settings)?;
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
        .line_numbers(settings.output.numbers)
        .print()
        .with_context(|| {"while trying to prettyprint yaml"})?;

    Ok(())
}

fn set_characteristic(id: &String, priority: &Option<TaskPriority>, due_date: &Option<NaiveDateTime>,
        tags: &Option<Vec<String>>, project: &Option<String>, metadata: &Option<Vec<MetadataKeyValuePair>>, settings: &Settings) -> Result<()> {
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

    if let Some(tags) = tags {
        let mut task_tags = if let Some(task_tags) = task.tags.clone() {
            task_tags
        } else {
            vec![]
        };

        let mut tags_modified = false;
        for new_tag in tags {
            if !task_tags.contains(new_tag) {
                task_tags.push(new_tag.to_string());
                tags_modified = true;
                println!("Tag '{}' added", new_tag);
            }
        }

        if tags_modified {
            task.tags = Some(task_tags);
            modified = true;
        }
    }

    if project.is_some() {
        task.project = project.clone();
        println!("Task now belongs to project '{}'", task.project.clone().unwrap());
        modified = true;
    }

    if let Some(metadata) = metadata {
        for new_metadata in metadata {
            let old = task.metadata.insert(new_metadata.key.clone(), new_metadata.value.clone());
            modified = true;
            if old.is_some() {
                println!("Metadata '{}' = '{}' updated", new_metadata.key, new_metadata.value);
            } else {
                println!("Metadata '{}' = '{}' added", new_metadata.key, new_metadata.value);
            }
        }
    }

    if modified {
        task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
        println!("Modifications saved for task '{}'", task.id);
    }

    Ok(())
}

fn unset_characteristic(id: &String, priority: &bool, due_date: &bool,
        tags: &Option<Vec<String>>, project: &bool, metadata: &Option<Vec<String>>, settings: &Settings) -> Result<()> {
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

    if *due_date {
        let old_duedate = task.metadata.remove("tsk-rs-task-due-time");
        if old_duedate.is_some() {
            modified = true;
            println!("Due date is now unset");
        }
    }

    if let Some(tags) = tags {
        let mut task_tags = if let Some(task_tags) = task.tags.clone() {
            task_tags
        } else {
            vec![]
        };

        let mut tags_modified = false;
        for remove_tag in tags {
            if let Some(index) = task_tags.iter().position(|r| r == remove_tag) {
                task_tags.swap_remove(index);
                println!("Tag '{}' removed", remove_tag);
                tags_modified = true;
            }
        }

        if tags_modified {
            task.tags = Some(task_tags);
            modified = true;
        }
    }

    if *project {
        task.project = None;
        println!("Task no longer is part of any project.");
        modified = true;
    }

    if let Some(metadata) = metadata {
        for remove_metadata in metadata {
            let old = task.metadata.remove(remove_metadata);
            if let Some(old) = old {
                println!("Metadata '{}' = '{}' removed", remove_metadata, old);
                modified = true;
            }
        }
    }

    if modified {
        task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate).with_context(|| {"while saving task yaml file"})?;
        println!("Modifications saved for task '{}'", task.id);
    }

    Ok(())
}

// eof
