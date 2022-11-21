use std::{collections::BTreeMap, fs::File, path::PathBuf, io::{Read, Write}, fmt::Display};
use file_lock::{FileOptions, FileLock};
use serde::{Serialize, Deserialize};
use simple_file_rotation::FileRotation;
use uuid::Uuid;
use anyhow::{Result, Context, bail};
use markdown::{self, mdast::Node};
use thiserror::Error;
use glob::glob;

use crate::{settings::Settings, metadata::MetadataKeyValuePair, task::{Task, task_pathbuf_from_id, load_task}};

#[derive(Error, Debug, PartialEq, Eq)]
pub enum NoteError {
    #[error("error while parsing action points")]
    ActionPointParseError,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Note {
    pub task_id: Uuid,
    pub markdown: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

impl Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_yaml_string().unwrap())
    }
}

impl Note {
    pub fn new(task_id: &Uuid) -> Self {

        let mut metadata: BTreeMap<String, String> = BTreeMap::new();
        let timestamp = chrono::offset::Local::now();
        metadata.insert(String::from("tsk-rs-note-create-time"), timestamp.to_rfc3339());

        Self {
            task_id: *task_id,
            markdown: None,
            metadata
        }
    }

    pub fn from_yaml_string(yaml_string: &str) -> Result<Self> {
        serde_yaml::from_str(yaml_string).with_context(|| {"while deserializing note yaml string"})
    }

    pub fn to_yaml_string(&self) -> Result<String> {       
        serde_yaml::to_string(self).with_context(|| {"while serializing note struct as YAML"})
    }

    pub fn load_yaml_file_from(note_pathbuf: &PathBuf) -> Result<Self> {
        let note: Note;
        {
            let mut file = File::open(note_pathbuf).with_context(|| {"while opening note yaml file for reading"})?;
            let mut note_yaml: String = String::new();
            file.read_to_string(&mut note_yaml).with_context(|| {"while reading note yaml file"})?;
            note = Note::from_yaml_string(&note_yaml).with_context(|| {"while serializing yaml into note struct"})?;
        }
        Ok(note)
    }

    pub fn save_yaml_file_to(&mut self, note_pathbuf: &PathBuf, rotate: &usize) -> Result<()> {
        // rotate existing file with same name if present
        if note_pathbuf.is_file() && rotate > &0 {
            FileRotation::new(&note_pathbuf).max_old_files(*rotate).file_extension("yaml".to_string()).rotate()
                .with_context(|| {"while rotating note data file backups"})?;
        }

        let should_we_block  = true;
        let options = FileOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .append(false);
        {
            let mut filelock= FileLock::lock(note_pathbuf, should_we_block, options)
                .with_context(|| {"while opening note yaml file"})?;
            filelock.file.write_all(self.to_yaml_string().with_context(|| {"while serializing note struct to yaml"})?.as_bytes()).with_context(|| {"while writing to note yaml file"})?;
            filelock.file.flush().with_context(|| {"while flushing os caches to disk"})?;
            filelock.file.sync_all().with_context(|| {"while syncing filesystem metadata"})?;
        }

        Ok(())
    }

    pub fn get_action_points(&self) -> Result<Option<Vec<ActionPoint>>> {
        if let Some(markdown_body) = self.markdown.clone() {
            let parse_result = markdown::to_mdast(&markdown_body, &markdown::ParseOptions::gfm());
            if parse_result.is_err() {
                panic!("error on parse")
            }
            let root_node = parse_result.unwrap();
            return parse_md_component(&self.task_id, &root_node);
        }
        Ok(None)
    }

    pub fn set_characteristic(&mut self, metadata: &Option<Vec<MetadataKeyValuePair>>) -> bool {   
        let mut modified = false;
    
        if let Some(metadata) = metadata {
            for new_metadata in metadata {
                self.metadata.insert(new_metadata.key.clone(), new_metadata.value.clone());
                modified = true;
            }
        }

        modified
    }

    pub fn unset_characteristic(&mut self, metadata: &Option<Vec<String>>) -> bool {
        let mut modified = false;
    
        if let Some(metadata) = metadata {
            for remove_metadata in metadata {
                let old = self.metadata.remove(remove_metadata);
                if old.is_some() {
                    modified = true;
                }
            }
        }    
  
        modified
    }

}

fn parse_md_component(task_id: &Uuid, node: &Node) -> Result<Option<Vec<ActionPoint>>> {
    let mut found_action_points = vec![];

    if let Some(child_nodes) = node.children() {
        for child_node in child_nodes {
            found_action_points.append(&mut parse_md_component(task_id, child_node)?.unwrap_or_default());
        }
    }

    if let Node::ListItem(list_node) = node {
        if list_node.checked.is_some() {
            let action_description_paragraphs = list_node.children.clone().pop().unwrap();
            let action_description = match action_description_paragraphs.children().unwrap().to_owned().pop().unwrap() {
                Node::Text(item_text) => item_text.value,
                _ => bail!(NoteError::ActionPointParseError)
            };
            found_action_points.push(ActionPoint{
                id: Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("tsk-rs://{}/{}", task_id, action_description).as_bytes()),
                description: action_description,
                checked: list_node.checked.unwrap(),
            });
        }
    }

    if found_action_points.is_empty() {
        Ok(None)
    } else {
        Ok(Some(found_action_points))
    }
}

#[derive(Debug)]
pub struct ActionPoint {
    pub id: Uuid,
    pub description: String,
    pub checked: bool,
}

pub fn note_pathbuf_from_id(id: &String, settings: &Settings) -> Result<PathBuf> {
    Ok(settings.note_db_pathbuf()?.join(PathBuf::from(format!("{}.yaml", id))))
}

pub fn note_pathbuf_from_note(note: &Note, settings: &Settings) -> Result<PathBuf> {
    note_pathbuf_from_id(&note.task_id.to_string(), settings)
}

pub fn load_note(id: &String, settings: &Settings) -> Result<Note> {
    let note_pathbuf = note_pathbuf_from_id(id, settings).with_context(|| {"while building path of the file"})?;
    let note = Note::load_yaml_file_from(&note_pathbuf).with_context(|| {"while loading note yaml file for editing"})?;
    Ok(note)
}

pub fn save_note(note: &mut Note, settings: &Settings) -> Result<()> {
    let note_pathbuf = note_pathbuf_from_note(note, settings)?;
    note.save_yaml_file_to(&note_pathbuf, &settings.data.rotate).with_context(|| {"while saving note yaml file"})?;
    Ok(())
}

pub struct FoundNote {
    pub note: Note,
    pub task: Option<Task>,
}

pub fn list_notes(id: &Option<String>, orphaned: &bool, completed: &bool, settings: &Settings) -> Result<Vec<FoundNote>> {
    let note_pathbuf: PathBuf = if id.is_some() {
        note_pathbuf_from_id(&format!("*{}*.yaml", id.as_ref().unwrap()), settings)?
    } else {
        note_pathbuf_from_id(&"*.yaml".to_string(), settings)?
    };

    let mut found_notes: Vec<FoundNote> = vec![];

    for note_filename in glob(note_pathbuf.to_str().unwrap()).with_context(|| {"while traversing note data directory files"})? {
        // if the filename is u-u-i-d.3.yaml for example it is a backup file and should be disregarded
        if note_filename.as_ref().unwrap().file_name().unwrap().to_string_lossy().split('.').collect::<Vec<_>>()[1] != "yaml" {
            continue;
        }

        let note = Note::load_yaml_file_from(&note_filename?).with_context(|| {"while loading note from disk"})?;

        let task_pathbuf = task_pathbuf_from_id(&note.task_id.to_string(), settings)?;
        let mut task: Option<Task> = None;
        if task_pathbuf.is_file() {
            task = Some(load_task(&note.task_id.to_string(), settings)?);
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
                found_notes.push(FoundNote {
                    note,
                    task: Some(task),
                });
            }
        } else if *orphaned {
            // there is no task file anymore, and orphaned is true so we add it anyway to the return value
            found_notes.push(FoundNote { note, task: None });
        }
    }

    Ok(found_notes)
}


#[cfg(test)]
mod tests {
    use chrono::{DateTime, Datelike};

    use super::*;

    static YAMLTESTINPUT: &str = "task_id: bd6f75aa-8c8d-47fb-b905-d9f7b15c782d\nmarkdown: fubar\nmetadata:\n  tsk-rs-note-create-time: 2022-08-06T07:55:26.568460389+00:00\n  x-fuu: bar\n";

    #[test]
    fn test_from_yaml() {
        let note = Note::from_yaml_string(YAMLTESTINPUT).unwrap();

        assert_eq!(note.task_id, Uuid::parse_str("bd6f75aa-8c8d-47fb-b905-d9f7b15c782d").unwrap());
        assert_eq!(note.markdown, Some("fubar".to_string()));

        let timestamp = DateTime::parse_from_rfc3339(note.metadata.get("tsk-rs-note-create-time").unwrap()).unwrap();
        assert_eq!(timestamp.year(), 2022);
        assert_eq!(timestamp.month(), 8);
        assert_eq!(timestamp.day(), 6);
    }

    #[test]
    fn test_to_yaml() {
        let mut note = Note::new(&Uuid::parse_str("bd6f75aa-8c8d-47fb-b905-d9f7b15c782d").unwrap());
        note.markdown = Some("fubar".to_string());
        note.metadata.insert("x-fuu".to_string(), "bar".to_string());
        // replace the create timestamp metadata to match test input
        note.metadata.insert("tsk-rs-note-create-time".to_string(), "2022-08-06T07:55:26.568460389+00:00".to_string());

        let yaml = note.to_yaml_string().unwrap();
        assert_eq!(yaml, YAMLTESTINPUT);
    }
}

// eof