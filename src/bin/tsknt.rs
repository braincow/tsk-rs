use std::path::PathBuf;

use anyhow::{Result, Context, bail};
use clap::{Parser, Subcommand};
use config::Config;
use tsk_rs::{settings::Settings, task::{Task, TaskError}, note::Note};
use edit::edit;

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
    Edit {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// mode selection
        #[clap(short, long, value_parser)]
        raw: bool,
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
        Some(Commands::Edit { id, raw }) => {
            edit_note(id, raw, &settings)
        },
        None => {todo!("we should list available notes here"); }
    }
}

fn edit_note(id: &String, raw: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for reading"})?;

    if task.is_done() {
        bail!(TaskError::TaskAlreadyCompleted);
    }

    let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut note: Note;
    if !note_pathbuf.is_file() {
        note = Note::new(&task.id);
        note.save_yaml_file_to(&note_pathbuf).with_context(|| {"while saving new task note file"})?;
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
    note.save_yaml_file_to(&note_pathbuf).with_context(|| {"while saving modified note yaml file"})?;

    Ok(())
}

// eof