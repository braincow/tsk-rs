use std::{path::PathBuf, fs::remove_file};
use anyhow::{Result, Context, bail};
use clap::{Parser, Subcommand};
use cli_table::{Cell, Table, Style, format::{Border, Separator}, print_stdout};
use question::{Question, Answer};
use tsk_rs::{settings::{Settings, show_config, default_config}, task::{TaskError, load_task}, note::{Note, load_note, save_note, note_pathbuf_from_id, note_pathbuf_from_note, list_notes, amount_of_notes}, metadata::MetadataKeyValuePair};
use bat::{Input, PrettyPrinter};
use dotenv::dotenv;
use termtree::Tree;

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
    /// Create or edit a note for an task
    #[clap(allow_missing_positional = true)]
    Edit {
        /// Existing task/note id
        #[clap(value_parser)]
        id: String,
        /// Edit raw YAML definition of the note (for advanced users)
        #[clap(short, long, value_parser)]
        raw: bool,
    },
    /// Read note or entire definition
    Show {
        /// Existing task/note id
        #[clap(value_parser)]
        id: String,
        /// Show raw YAML notation of the note instead of Markdown
        #[clap(short, long, value_parser)]
        raw: bool,
    },
    /// Ddelete a note
    Delete {
        /// Existing task/note id
        #[clap(value_parser)]
        id: String,
        /// Delete file silently
        #[clap(short, long, value_parser)]
        force: bool,
    },
    /// List note(s)
    List {
        /// Existing task/note id or a part of one. Empty will list all.
        #[clap(value_parser)]
        id: Option<String>,
        /// List orphaned notes (Task file has been deleted)
        #[clap(short, long, value_parser)]
        orphaned: bool,
        /// List notes for completed tasks
        #[clap(short, long, value_parser)]
        completed: bool,
    },
    /// List action point(s) from task notes
    #[clap(visible_alias="aps")]
    ActionPoints {
        /// Existing task/note id or a part of one. Empty will list all.
        #[clap(value_parser)]
        id: Option<String>,
        /// List aps from orphaned notes (Task file has been deleted)
        #[clap(short, long, value_parser)]
        orphaned: bool,
        /// List aps from notes for completed tasks
        #[clap(short, long, value_parser)]
        completed: bool,
        /// List aps that are done
        #[clap(short, long, value_parser)]
        done: bool,        
    },
    /// Display the current configuration of the tsk-rs suite
    Config,
    /// Set note characteristics
    Set {
        /// Existing task/note id
        #[clap(value_parser)]
        id: String,
        /// Add metadata from note: x-key=value
        #[clap(long,value_parser)]
        metadata: Option<Vec<MetadataKeyValuePair>>,
    },
    /// Unset note characteristics
    Unset {
        /// Existing task/note id
        #[clap(value_parser)]
        id: String,
        /// Remove metadata(s) from note: x-key
        #[clap(long,value_parser)]
        metadata: Option<Vec<String>>,
    }
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
        Some(Commands::Edit { id, raw }) => {
            edit_note(id, raw, &settings)
        },
        Some(Commands::Show { id, raw }) => {
            show_note(id, raw, &settings)
        },
        Some(Commands::List {id, orphaned, completed }) => {
            cli_list_notes(id, orphaned, completed, &settings)
        },
        Some(Commands::Delete { id, force }) => {
            delete_note(id, force, &settings)
        },
        Some(Commands::Config) => {
            show_config(&settings)
        },
        Some(Commands::Set { id, metadata }) => {
            cli_set_characteristic(id, metadata, &settings)
        },
        Some(Commands::Unset { id, metadata }) => {
            cli_unset_characteristic(id, metadata, &settings)
        },
        Some(Commands::ActionPoints { id, orphaned, completed, done }) => {
            list_aps(id, orphaned, completed, done, &settings)
        },
        None => { cli_list_notes(&None, &false, &false, &settings) }
    }
}

fn list_aps(id: &Option<String>, orphaned: &bool, completed: &bool, done: &bool, settings: &Settings) -> Result<()> {
    let found_notes = list_notes(id, orphaned, completed, settings)?;

    let mut tree_root = Tree::new("🗐 Task notes".to_string());
    let mut tree_populated = false;

    for found_note in found_notes {
        let aps = found_note.note.get_action_points()?;
        if let Some(aps) = aps {
            let desc = if let Some(task) = found_note.task.clone() {
                let mut desc = task.description.clone();
                if desc.len() > settings.output.descriptionlength + 3 {
                    // if the desc truncated to max length plus three dot characters is
                    //  shorter than the max len then truncate it and add those three dots
                    desc = format!("{}...", &desc[..settings.output.descriptionlength]);
                }
                desc
            } else {
                "[ orphaned ]".to_string()
            };
    
            let task_id = if let Some(task) = found_note.task {
                task.id.to_string()
            } else {
                "[ orphaned ]".to_string()
            };
    
            let mut ap_added_to_leaf = false;
            let mut note_leaf = Tree::new(format!("🗏 {} | {}", desc, task_id));
            for ap in aps {
                if *done || !ap.checked {
                    let mark = if ap.checked {
                        "🗹"
                    } else {
                        "☐"
                    };
                    let action_leaf = Tree::new(format!("{} {}", mark, ap.description));
                    note_leaf.push(action_leaf);
                    ap_added_to_leaf = true;
                }
            }
            if ap_added_to_leaf {
                tree_root.push(note_leaf);
                tree_populated = true;    
            }
        }    
    }

    if tree_populated {
        println!("\n{}", tree_root);
    } else {
        println!("No action points.");
    }

    Ok(())
}

fn cli_list_notes(id: &Option<String>, orphaned: &bool, completed: &bool, settings: &Settings) -> Result<()> {
    let mut note_cells = vec![];

    let found_notes = list_notes(id, orphaned, completed, settings)?;
    let found_notes_count: usize = amount_of_notes(settings, false)?;

    let mut listed_notes_count: usize = 0;
    for found_note in found_notes {
        if let Some(task) = found_note.task {
            let mut desc = task.description.clone();
            if desc.len() > settings.output.descriptionlength + 3 {
                // if the desc truncated to max length plus three dot characters is
                //  shorter than the max len then truncate it and add those three dots
                desc = format!("{}...", &desc[..settings.output.descriptionlength]);
            }
            listed_notes_count += 1;
            note_cells.push(vec![task.id.cell(), desc.cell(),
                task.project.unwrap_or_else(|| {"".to_string()}).cell(),]);
        } else if *orphaned {
            // there is no task file anymore, and orphaned is true so we add it
            note_cells.push(vec![
                found_note.note.task_id.cell(),
                "[orphaned]".to_string().cell(),
                "[orphaned]".to_string().cell(),
                "[orphaned]".to_string().cell(),
            ]);
        }
    }

    if !note_cells.is_empty() {
        let tasks_table = note_cells.table()
            .title(
                vec!["Note/Task ID".cell().bold(true).underline(true),
                "Description".cell().bold(true).underline(true),
                "Project".cell().bold(true).underline(true)]) // headers of the table
            .border(Border::builder().build())
            .separator(Separator::builder().build()); // empty border around the table
            print_stdout(tasks_table).with_context(|| {"while trying to print out pretty table of task(s)"})?;

        println!("\n Number of notes: {}/{}", listed_notes_count, found_notes_count);
    } else {
        println!("No task notes");
    }

    Ok(())
}

fn delete_note(id: &String, force: &bool, settings: &Settings) -> Result<()> {
    let note = load_note(id, settings)?;
    let note_pathbuf = note_pathbuf_from_note(&note, settings)?;

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

fn edit_note(id: &String, raw: &bool, settings: &Settings) -> Result<()> {
    let task = load_task(id, settings)?;
    if task.done {
        bail!(TaskError::TaskAlreadyCompleted);
    }

    let mut modified = false;
    let mut note;
    let note_pathbuf = note_pathbuf_from_id(id, settings)?;
    if !note_pathbuf.is_file() {
        note = Note::new(&task.id);
    } else {
        note = load_note(id, settings)?;
    }

    if !raw {
        // by default we edit only the Markdown notation inside the file
        let mut md: String = note.markdown.clone().unwrap_or_default();
        if md.is_empty() && settings.note.description {
            md = format!("# {}\n\n", task.description);
        }
        if settings.note.timestamp {
            let local_timestamp = chrono::offset::Local::now();
            md = format!("{}\n## {}\n\n\n", md, local_timestamp);
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
        save_note(&mut note, settings).with_context(|| {"while saving note yaml file"})?;
        println!("Note for '{}' was updated.", note.task_id);
    }

    Ok(())
}

fn show_note(id: &String, raw: &bool, settings: &Settings) -> Result<()> {
    let note = load_note(id, settings)?;

    if !raw {
        // by default, only show the markdown inside the note yaml
        if let Some(md) = note.markdown {
            PrettyPrinter::new()
                .language("markdown")
                .input(Input::from_bytes(md.as_bytes()))
                .colored_output(settings.output.colors)
                .grid(settings.output.grid)
                .line_numbers(settings.output.numbers)
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
            .line_numbers(settings.output.numbers)
            .print()
            .with_context(|| {"while trying to prettyprint yaml"})?;
    }

    Ok(())
}

fn cli_set_characteristic(id: &String, metadata: &Option<Vec<MetadataKeyValuePair>>, settings: &Settings) -> Result<()> {
    let mut note = load_note(id, settings)?;
    let modified = note.set_characteristic(metadata);

    if modified {
        save_note(&mut note, settings)?;
        println!("Modifications saved for note '{}'", note.task_id);
    }

    Ok(())
}

fn cli_unset_characteristic(id: &String, metadata: &Option<Vec<String>>, settings: &Settings) -> Result<()> {
    let mut note = load_note(id, settings)?;
    let modified = note.unset_characteristic(metadata);

    if modified {
        save_note(&mut note, settings)?;
        println!("Modifications saved for note '{}'", note.task_id);
    }

    Ok(())
}

// eof