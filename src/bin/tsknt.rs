use std::{path::PathBuf, fs::remove_file};
use anyhow::{Result, Context, bail};
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, format::{Border, Separator}, print_stdout};
use question::{Question, Answer};
use tsk_rs::{settings::{Settings, show_config}, task::{Task, TaskError}, note::Note, metadata::MetadataKeyValuePair};
use glob::glob;
use bat::{Input, PrettyPrinter};

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
    /// Create or edit a note for an task
    #[clap(allow_missing_positional = true)]
    Edit {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// mode selection
        #[clap(short, long, value_parser)]
        raw: bool,
    },
    /// Read note or entire definition
    Show {
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
    List {
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
    /// display the current configuration of the tsk-rs suite
    Config,
    /// Set note characteristics
    Set {
        /// task id
        #[clap(value_parser)]
        id: String,
        #[clap(long,value_parser)]
        metadata: Option<Vec<MetadataKeyValuePair>>,
    },
    /// Unset note characteristics
    Unset {
        /// task id
        #[clap(value_parser)]
        id: String,
        /// remove metadata from note
        #[clap(long,value_parser)]
        metadata: Option<Vec<String>>,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let settings = Settings::new(cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    match &cli.command {
        Some(Commands::Edit { id, raw }) => {
            edit_note(id, raw, &settings)
        },
        Some(Commands::Show { id, raw }) => {
            show_note(id, raw, &settings)
        },
        Some(Commands::List {id, orphaned, completed }) => {
            list_note(id, orphaned, completed, &settings)
        },
        Some(Commands::Delete { id, force }) => {
            delete_note(id, force, &settings)
        },
        Some(Commands::Config) => {
            show_config(&settings)
        },
        Some(Commands::Set { id, metadata }) => {
            set_characteristic(id, metadata, &settings)
        },
        Some(Commands::Unset { id, metadata }) => {
            unset_characteristic(id, metadata, &settings)
        },

        None => { list_note(&None, &false, &false, &settings) }
    }
}

fn list_note(id: &Option<String>, orphaned: &bool, completed: &bool, settings: &Settings) -> Result<()> {
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
                vec!["Note ID".cell().bold(true).underline(true),
                "Description".cell().bold(true).underline(true),
                "Project".cell().bold(true).underline(true)]) // headers of the table
            .border(Border::builder().build())
            .separator(Separator::builder().build()); // empty border around the table
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
    } else {
        println!("Cancelled: Note for '{}' not deleted.", note.task_id);    
    }

    Ok(())
}

fn edit_note(id: &String, raw: &bool, settings: &Settings) -> Result<()> {
    let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let task = Task::load_yaml_file_from(&task_pathbuf).with_context(|| {"while loading task yaml file for reading"})?;

    if task.done {
        bail!(TaskError::TaskAlreadyCompleted);
    }

    let mut modified = false;
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
        let mut md: String = note.markdown.clone().unwrap_or_default();
        if md.is_empty() && settings.note.add_description_on_new {
            md = format!("# {}\n\n", task.description);
        }
        if settings.note.add_timestamp_on_edit {
            let local_timestamp = chrono::offset::Local::now();
            md = format!("{}## {}\n\n", md, local_timestamp);
        }

        let new_md = edit::edit_with_builder(md.clone(), edit::Builder::new().suffix(".md")).with_context(|| {"while starting an external editor"})?;

        if new_md != md {
            note.markdown = Some(new_md);
            modified = true;
        }
    } else {
        // modify the raw YAML notation of the task file
        let new_yaml = edit::edit_with_builder(note.to_yaml_string()?, edit::Builder::new().suffix(".yaml")).with_context(|| {"while starting an external editor"})?;
        if new_yaml != note.to_yaml_string()? {
            note = Note::from_yaml_string(&new_yaml).with_context(|| {"while deserializing modified note yaml"})?;
            modified = true;
        }
    }

    if modified {
        note.save_yaml_file_to(&note_pathbuf, &settings.data.rotate).with_context(|| {"while saving modified note yaml file"})?;
        println!("Note for '{}' was updated.", note.task_id);
    } else {
        println!("No updates made to note for '{}'.", note.task_id);
    }

    Ok(())
}

fn show_note(id: &String, raw: &bool, settings: &Settings) -> Result<()> {
    let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let note = Note::load_yaml_file_from(&note_pathbuf).with_context(|| {"while loading note from disk"})?;

    if !raw {
        // by default, only show the markdown inside the note yaml
        if let Some(md) = note.markdown {
            PrettyPrinter::new()
                .language("markdown")
                .input(Input::from_bytes(md.as_bytes()))
                .colored_output(settings.output.colors)
                .grid(settings.output.grid)
                .print()
                .with_context(|| {"while trying to prettyprint markdown"})?;
        }
    } else {
        let note_yaml = note.to_yaml_string()?;
        PrettyPrinter::new()
            .language("yaml")
            .input(Input::from_bytes(note_yaml.as_bytes()))
            .colored_output(settings.output.colors)
            .grid(settings.output.grid)
            .print()
            .with_context(|| {"while trying to prettyprint yaml"})?;
    }

    Ok(())
}

fn set_characteristic(id: &String, metadata: &Option<Vec<MetadataKeyValuePair>>, settings: &Settings) -> Result<()> {
    let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut note = Note::load_yaml_file_from(&note_pathbuf).with_context(|| {"while loading note from disk"})?;

    let mut modified = false;

    if let Some(metadata) = metadata {
        for new_metadata in metadata {
            let old = note.metadata.insert(new_metadata.key.clone(), new_metadata.value.clone());
            modified = true;
            if old.is_some() {
                println!("Metadata '{}' = '{}' updated", new_metadata.key, new_metadata.value);
            } else {
                println!("Metadata '{}' = '{}' added", new_metadata.key, new_metadata.value);
            }
        }
    }

    if modified {
        note.save_yaml_file_to(&note_pathbuf, &settings.data.rotate).with_context(|| {"while saving note yaml file"})?;
        println!("Modifications saved for note '{}'", note.task_id);
    }

    Ok(())
}

fn unset_characteristic(id: &String, metadata: &Option<Vec<String>>, settings: &Settings) -> Result<()> {
    let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id)));
    let mut note = Note::load_yaml_file_from(&note_pathbuf).with_context(|| {"while loading note from disk"})?;

    let mut modified = false;

    if let Some(metadata) = metadata {
        for remove_metadata in metadata {
            let old = note.metadata.remove(remove_metadata);
            if let Some(old) = old {
                println!("Metadata '{}' = '{}' removed", remove_metadata, old);
                modified = true;
            }
        }
    }

    if modified {
        note.save_yaml_file_to(&note_pathbuf, &settings.data.rotate).with_context(|| {"while saving note yaml file"})?;
        println!("Modifications saved for note '{}'", note.task_id);
    }

    Ok(())
}



// eof