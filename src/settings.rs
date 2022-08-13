use std::{path::PathBuf, fs::create_dir_all, fmt::Display};
use anyhow::{Result, Context};
use config::Config;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    pub colors: bool,
    pub grid: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self { colors: true, grid: true }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteSettings {
    pub add_description_on_new: bool,
    pub add_timestamp_on_edit: bool,
}

impl Default for NoteSettings {
    fn default() -> Self {
        Self { add_description_on_new: true, add_timestamp_on_edit: true }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DataSettings {
    pub db_path: String,
    pub rotate: usize,
}

impl Default for DataSettings {
    fn default() -> Self {
        let proj_dirs = ProjectDirs::from("", "",  "tsk-rs").unwrap();

        Self {
            db_path: String::from(proj_dirs.data_dir().to_str().unwrap()),
            rotate: 3,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub data: DataSettings,
    pub note: NoteSettings,
    pub output: OutputSettings,
}

impl Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", toml::to_string(&self).unwrap())
    }
}

impl Settings {
    pub fn new(config_file: &str) -> Result<Self> {
        let mut config = Config::builder();
        if PathBuf::from(config_file).is_file() {
            config = config.add_source(config::File::with_name(config_file));
        }
        config = config.add_source(config::Environment::with_prefix("TSK"));
        let ready_config = config.build().with_context(|| {"while reading configuration"})?;
        let settings: Settings = ready_config.try_deserialize().with_context(|| {"while applying defaults to configuration"})?;

        Ok(settings)
    }

    pub fn db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = PathBuf::from(&self.data.db_path);
        if !pathbuf.is_dir() {
            create_dir_all(&pathbuf).with_context(|| {"while creating data directory"})?;
        }
        Ok(pathbuf)
    }

    pub fn task_db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = &self.db_pathbuf()?.join("tasks");
        if !pathbuf.is_dir() {
            create_dir_all(&pathbuf).with_context(|| {"while creating tasks data directory"})?;
        }
        Ok(pathbuf.to_path_buf())
    }

    pub fn note_db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = &self.db_pathbuf()?.join("notes");
        if !pathbuf.is_dir() {
            create_dir_all(&pathbuf).with_context(|| {"while creating notes data directory"})?;
        }
        Ok(pathbuf.to_path_buf())
    }

}

// eof
