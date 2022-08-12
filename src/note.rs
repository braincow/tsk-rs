use std::{collections::BTreeMap, fs::File, path::PathBuf, io::{Read, Write}};

use file_lock::{FileOptions, FileLock};
use serde::{Serialize, Deserialize};
use simple_file_rotation::FileRotation;
use uuid::Uuid;
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize)]
pub struct Note {
    pub task_id: Uuid,
    pub markdown: Option<String>,
    pub metadata: Option<BTreeMap<String, String>>,
}

impl Note {
    pub fn new(task_id: &Uuid) -> Self {

        let mut metadata: BTreeMap<String, String> = BTreeMap::new();
        let timestamp = chrono::offset::Utc::now();
        metadata.insert(String::from("tsk-rs-note-create-time"), timestamp.to_rfc3339());

        Self {
            task_id: *task_id,
            markdown: None,
            metadata: Some(metadata)
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
            FileRotation::new(&note_pathbuf).max_old_files(*rotate).rotate()?;
        }

        let should_we_block  = true;
        let options = FileOptions::new()
            .write(true)
            .create(true)
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


}

// eof