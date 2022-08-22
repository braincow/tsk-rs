use std::{path::PathBuf, fs::create_dir_all, fmt::Display};
use anyhow::{Result, Context, bail};
use bat::{PrettyPrinter, Input};
use config::Config;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SettingsError {
    #[error("namespace cannot be empty")]
    EmptyNamespaceNotAllowed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskSettings {
    pub release_hold_on_start: bool,
    pub enable_start_special_tag: bool,
    pub show_special_tags_on_list: bool,
    pub stop_tracking_when_done: bool,
    pub remove_special_tags_on_done: bool,
}

impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            release_hold_on_start: true,
            enable_start_special_tag: true,
            show_special_tags_on_list: true,
            stop_tracking_when_done: true,
            remove_special_tags_on_done: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    pub colors: bool,
    pub grid: bool,
    pub line_numbers: bool,
    pub show_namespace: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self { colors: true, grid: true, line_numbers: true, show_namespace: true }
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
    #[serde(skip_serializing)]
    pub namespace: String,
    pub data: DataSettings,
    pub note: NoteSettings,
    pub task: TaskSettings,
    pub output: OutputSettings,
}

impl Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", toml::to_string(&self).unwrap())
    }
}

impl Settings {
    pub fn new(namespace: String, config_file: &str) -> Result<Self> {
        let mut settings: Settings = Config::builder()
            .add_source(config::File::with_name(config_file).required(false))
            .add_source(config::Environment::with_prefix("TSK").try_parsing(true).separator("_"))
            .build().with_context(|| {"while reading configuration"})?
            .try_deserialize().with_context(|| {"while applying defaults to configuration"})?;

        if !namespace.is_empty() && settings.namespace.is_empty() {
            // namespace can come from env as well so only set it if it was not read from env by config crate
            settings.namespace = namespace;
        }
        if settings.namespace.is_empty() {
            bail!(SettingsError::EmptyNamespaceNotAllowed);
        }

        Ok(settings)
    }

    pub fn db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = PathBuf::from(&self.data.db_path).join(&self.namespace);
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

pub fn show_config(settings: &Settings) -> Result<()> {
    let settings_toml = format!("{}", settings);
    PrettyPrinter::new()
        .language("toml")
        .input(Input::from_bytes(settings_toml.as_bytes()))
        .colored_output(settings.output.colors)
        .grid(settings.output.grid)
        .line_numbers(settings.output.line_numbers)
        .print()
        .with_context(|| {"while trying to prettyprint yaml"})?;

    Ok(())
}

// eof
