use std::path::PathBuf;
use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tantivy::{Index, Document, ReloadPolicy, query::QueryParser, collector::TopDocs};
use tsk_rs::{settings::{Settings, show_config}, index::{task_schema, note_schema}, task::Task, note::Note};
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
    /// display the current configuration of the tsk-rs suite
    Config,
    /// rebuild search index(es)
    Rebuild {
        /// skip task index
        #[clap(long,value_parser)]
        skip_task: bool,
        /// skip note index
        #[clap(long,value_parser)]
        skip_note: bool,    
    },
    Search {
        /// search phrase
        #[clap(raw = true, value_parser)]
        phrase: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let settings = Settings::new(cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    match &cli.command {
        Some(Commands::Config) => {
            show_config(&settings)
        },
        Some(Commands::Rebuild { skip_task, skip_note }) => {
            rebuild_indexes(skip_task, skip_note, &settings)
        },
        Some(Commands::Search { phrase }) => { 
            search(phrase.join(" "), &settings)
        },
        None => {todo!()}
    }
}

fn rebuild_indexes(skip_note: &bool, skip_task: &bool, settings: &Settings) -> Result<()> {
    // https://github.com/quickwit-oss/tantivy/blob/main/examples/basic_search.rs
    if !skip_task {
        let mut task_index_path = settings.task_index_db_pathbuf()?;
        if task_index_path.is_dir() {
            std::fs::remove_dir_all(task_index_path.clone()).with_context(|| {"while deleting existing Task index"})?;
            task_index_path = settings.task_index_db_pathbuf()?;
            println!("Existing task index erased.")
        }

        let task_schema = task_schema();
        let index = Index::create_in_dir(&task_index_path, task_schema.clone())
            .with_context(|| {"while setting up task search index directory"})?;
        let mut writer = index.writer(50_000_000).with_context(|| {"while preparing index writer"})?;

        let task_id = task_schema.get_field("ID").with_context(|| {"get index field 'ID'"})?;
        let task_description = task_schema.get_field("description").with_context(|| {"get index field 'description'"})?;
        let task_project = task_schema.get_field("project").with_context(|| {"get index field 'project'"})?;
        // todo: index timetrack annotations
        // todo: index tags
        // todo: index metadata fields

        let task_pathbuf = settings.task_db_pathbuf()?.join(PathBuf::from("*.yaml"));
        for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
            let task = Task::load_yaml_file_from(&task_filename?).with_context(|| {"while loading task yaml file"})?;
            let mut task_doc = Document::default();
            task_doc.add_text(task_id, task.id);
            task_doc.add_text(task_description, task.description.clone());
            if let Some(project_name) = task.project {
                task_doc.add_text(task_project, project_name);
            }
            writer.add_document(task_doc).with_context(|| {"while adding Task document to index"})?;
            println!("Task '{}' added to index.", task.id);
        }
        writer.commit().with_context(|| {"while writing Task index"})?;
    } else {
        println!("Not rebuilding Task index.");
    }

    if !skip_note {
        let mut note_index_path = settings.note_index_db_pathbuf()?;
        if note_index_path.is_dir() {
            std::fs::remove_dir_all(note_index_path.clone()).with_context(|| {"while deleting existing Note index"})?;
            note_index_path = settings.note_index_db_pathbuf()?;
            println!("Existing note index erased.")
        }

        let note_schema = note_schema();
        let index = Index::create_in_dir(&note_index_path, note_schema.clone())
            .with_context(|| {"while setting up note search index directory"})?;
        let mut writer = index.writer(50_000_000).with_context(|| {"while preparing index writer"})?;

        let note_id = note_schema.get_field("ID").with_context(|| {"get index field 'ID'"})?;
        let note_markdown = note_schema.get_field("markdown").with_context(|| {"get index field 'markdown'"})?;
        // todo: index metadata fields

        let note_pathbuf = settings.note_db_pathbuf()?.join(PathBuf::from("*.yaml"));
        for note_filename in glob(note_pathbuf.to_str().unwrap()).with_context(|| {"while traversing note data directory files"})? {
            let note = Note::load_yaml_file_from(&note_filename?).with_context(|| {"while loading note yaml file"})?;
            let mut note_doc = Document::default();
            note_doc.add_text(note_id, note.task_id);
            if let Some(markdown) = note.markdown {
                note_doc.add_text(note_markdown, markdown);
            }
            writer.add_document(note_doc).with_context(|| {"while adding Note document to index"})?;
            println!("Note '{}' added to index.", note.task_id);
        }
        writer.commit().with_context(|| {"while writing Note index"})?;
    } else {
        println!("Not rebuilding Note index.");
    }

    Ok(())
}

fn search(phrase: String, settings: &Settings) -> Result<()> {
    // todo: search in task index
    let task_index_path = settings.task_index_db_pathbuf()?;
    let task_index = Index::open_in_dir(task_index_path).with_context(|| {"while opening Task index"})?;
    let task_reader = task_index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommit)
        .try_into().with_context(|| {"while building Task index reader"})?;
    let task_searcher = task_reader.searcher();
    let task_schema = task_schema();
    let task_description = task_schema.get_field("description").with_context(|| {"get index field 'description'"})?;
    let task_project = task_schema.get_field("project").with_context(|| {"get index field 'project'"})?;
    let task_query_parser = QueryParser::for_index(&task_index, vec![task_description, task_project]);
    let task_query = task_query_parser.parse_query(&phrase).with_context(|| {"while parsing search phrase"})?;
    let task_top_docs = task_searcher.search(&task_query, &TopDocs::with_limit(10)).with_context(|| {"while executing a search into Task index"})?;
    for (_score, doc_address) in task_top_docs {
        let retrieved_doc = task_searcher.doc(doc_address)?;
        println!("{}", task_schema.to_json(&retrieved_doc));
        println!("{}", _score);
        println!("{:?}", doc_address);
    }

    // todo: search in note index
    Ok(())
}

// eof