use std::{path::PathBuf, fs::create_dir_all};
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub db_path: String,
}

impl Default for Data {
    fn default() -> Self {
        let proj_dirs = ProjectDirs::from("", "",  "tsk-rs").unwrap();

        Self {
            db_path: String::from(proj_dirs.data_dir().to_str().unwrap())
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub data: Data,
}

impl Settings {
    pub fn db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = PathBuf::from(&self.data.db_path);
        if !pathbuf.is_dir() {
            create_dir_all(&pathbuf)?;
        }
        Ok(pathbuf)
    }
}
