use std::{path::PathBuf, fs::remove_file};

use anyhow::{Result, Context, bail};
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, format::Border, print_stdout};
use question::{Question, Answer};
use tsk_rs::{settings::Settings, task::{Task, TaskError}, note::Note};
use edit::edit;
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
    /// adds a new task from task description string
    #[clap(allow_missing_positional = true)]
    Jot {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// mode selection
        #[clap(short, long, value_parser)]
        raw: bool,
    },
    /// delete a note file
    Delete {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// delete file silently
        #[clap(short, long, value_parser)]
        force: bool,
    },
    /// show note(s)
    Show {
        /// task id
        #[clap(value_parser)]
        id: Option<String>,
        /// show orphaned notes (task file has been deleted)
        #[clap(short, long, value_parser)]
        orphaned: bool,
        /// show notes for completed tasks
        #[clap(short, long, value_parser)]
        completed: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let settings = Settings::new(cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    match &cli.command {
        Some(Commands::Jot { id, raw }) => {
            jot_note(id, raw, &settings)
        },
        Some(Commands::Show {id, orphaned, completed }) => {
            show_note(id, orphaned, completed, &settings)
        },
        Some(Commands::Delete { id, force }) => {
            delete_note(id, force, &settings)
        }
        None => { show_note(&None, &false, &false, &settings) }
    }
}

fn show_note(id: &Option<String>, orphaned: &bool, completed: &bool, settings: &Settings) -> Result<()> {
    let mut note_cells = vec![];

    let mut note_pathbuf: PathBuf = settings.note_db_pathbuf().with_context(|| {"invalid data directory path configured"})?;
    if id.is_some() {
        note_pathbuf = note_pathbuf.join(format!("*{}*.yaml", id.as_ref().unwrap()));
    } else {
        note_pathbuf = note_pathbuf.join("*.yaml");
    }
    for note_filename in glob(note_pathbuf.to_str().unwrap()).with_context(|| {"while traversing note data directory files"})? {
        let note = Note::load_yaml_file_from(&note_filename?).with_context(|| {"while loading note from disk"})?;

        let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", note.task_id)));
        let mut task: Option<Task> = None;
        if task_pathbuf.is_file() {
            task = Some(Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task from yaml file"})?);
        }

        if let Some(task) = task {
            let mut show_note = false;
            // there is a task file
            if task.done && *completed {
                // .. but the task is completed. however completed is true so we show it
                show_note = true;
            }
            if !task.done {
                // .. task is not done so show it
                show_note = true;
            }
            if show_note {
                note_cells.push(vec![note.task_id.cell(), task.description.cell(),
                    task.project.unwrap_or_else(|| {"".to_string()}).cell(),]);
            }
        } else if *orphaned {
            // there is no task file anymore, and orphaned is true so we add it
            note_cells.push(vec![
                note.task_id.cell(),
                "[orphaned]".to_string().cell(),
                "[orphaned]".to_string().cell(),
                "[orphaned]".to_string().cell(),
            ]);
        }
    }

    if !note_cells.is_empty() {
        let tasks_table = note_cells.table()
            .title(
                vec!["ID".cell().bold(true),
                "Description".cell().bold(true),
                "Project".cell().bold(true)]) // headers of the table
            .border(Border::builder().build()); // empty border around the table
        print_stdout(tasks_table).with_context(|| {"while trying to print out pretty table of task(s)"})?;
    } else {
        println!("No task notes");
    }

    Ok(())
}

fn delete_note(id: &String, force: &bool, settings: &Settings) -> Result<()> {
    let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));

    let note = Note::load_yaml_file_from(&note_pathbuf)?;
    
    let answer = if !force {
        Question::new("Really delete this note?")
        .default(Answer::NO)
        .show_defaults()
        .confirm()
    } else {
        Answer::YES
    };

    if answer == Answer::YES {
        remove_file(note_pathbuf).with_context(|| {"while removing note file"})?;
        println!("Note for '{}' now deleted permanently.", note.task_id);    
    }

    Ok(())
}


fn jot_note(id: &String, raw: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for reading"})?;

    if task.done {
        bail!(TaskError::TaskAlreadyCompleted);
    }

    let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut note: Note;
    if !note_pathbuf.is_file() {
        note = Note::new(&task.id);
        note.save_yaml_file_to(&note_pathbuf, &settings.data.rotate).with_context(|| {"while saving new task note file"})?;
    } else {
        note = Note::load_yaml_file_from(&note_pathbuf)?;
    }

    if !raw {
        // by default we edit only the Markdown notation inside the file
        let mut md: String = note.markdown.unwrap_or_default();
        if md.is_empty() && settings.note.add_description_on_new {
            md = format!("# {}\n\n", task.description);
        }
        if settings.note.add_timestamp_on_edit {
            let utc_timestamp = chrono::offset::Utc::now();
            md = format!("{}## {}\n\n", md, utc_timestamp);
        }
        md = edit(md).with_context(|| {"while starting an external editor"})?;
        note.markdown = Some(md);
    } else {
        // modify the raw YAML notation of the task file
        let new_yaml = edit(note.to_yaml_string()?).with_context(|| {"while starting an external editor"})?;
        note = Note::from_yaml_string(&new_yaml).with_context(|| {"while deserializing modified note yaml"})?;
    }
    note.save_yaml_file_to(&note_pathbuf, &settings.data.rotate).with_context(|| {"while saving modified note yaml file"})?;

    Ok(())
}

// eof