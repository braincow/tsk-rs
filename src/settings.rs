use std::{path::PathBuf, fs::create_dir_all, fmt::Display};
use anyhow::{Result, Context, bail};
use bat::{PrettyPrinter, Input};
use config::Config;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SettingsError {
    #[error("data directory does not exist, and createdir is set to false")]
    DataDirectoryDoesNotExist,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskSettings {
    pub autorelease: bool,
    pub starttag: bool,
    pub specialvisible: bool,
    pub stopondone: bool,
    pub clearpsecialtags: bool,
}

impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            autorelease: true,
            starttag: true,
            specialvisible: true,
            stopondone: true,
            clearpsecialtags: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    pub colors: bool,
    pub grid: bool,
    pub numbers: bool,
    pub namespace: bool,
    pub descriptionlength: usize,
    pub totals: bool,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            colors: true,
            grid: true,
            numbers: true,
            namespace: true,
            descriptionlength: 60,
            totals: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteSettings {
    pub description: bool,
    pub timestamp: bool,
}

impl Default for NoteSettings {
    fn default() -> Self {
        Self { description: true, timestamp: true }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DataSettings {
    pub path: String,
    pub createdir: bool,
    pub rotate: usize,
}

impl Default for DataSettings {
    fn default() -> Self {
        let proj_dirs = ProjectDirs::from("", "",  "tsk-rs").unwrap();

        Self {
            path: String::from(proj_dirs.data_dir().to_str().unwrap()),
            createdir: true,
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
    pub fn new(namespace: Option<String>, config_file: &str) -> Result<Self> {
        let mut settings: Settings = Config::builder()
            .set_override_option("namespace", namespace)?
            .add_source(config::File::with_name(config_file).required(false))
            .add_source(config::Environment::with_prefix("TSK").try_parsing(true).separator("_"))
            .build().with_context(|| {"while reading configuration"})?
            .try_deserialize().with_context(|| {"while applying defaults to configuration"})?;

        if settings.namespace.is_empty() {
            // namespace was not given from env or command line so set it to default
            settings.namespace = "default".to_string();
        }

        Ok(settings)
    }

    pub fn db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = PathBuf::from(&self.data.path).join(&self.namespace);
        if !pathbuf.is_dir() && self.data.createdir {
            create_dir_all(&pathbuf).with_context(|| {"while creating data directory"})?;
        } else if !pathbuf.is_dir() && !self.data.createdir {
            bail!(SettingsError::DataDirectoryDoesNotExist);
        }
        Ok(pathbuf)
    }

    pub fn task_db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = &self.db_pathbuf()?.join("tasks");
        if !pathbuf.is_dir() && self.data.createdir {
            create_dir_all(&pathbuf).with_context(|| {"while creating tasks data directory"})?;
        } else if !pathbuf.is_dir() && !self.data.createdir {
            bail!(SettingsError::DataDirectoryDoesNotExist);
        }
        Ok(pathbuf.to_path_buf())
    }

    pub fn note_db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = &self.db_pathbuf()?.join("notes");
        if !pathbuf.is_dir() && self.data.createdir {
            create_dir_all(&pathbuf).with_context(|| {"while creating notes data directory"})?;
        } else if !pathbuf.is_dir() && !self.data.createdir {
            bail!(SettingsError::DataDirectoryDoesNotExist);
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
        .line_numbers(settings.output.numbers)
        .print()
        .with_context(|| {"while trying to prettyprint yaml"})?;

    Ok(())
}

pub fn default_config() -> String {
    let proj_dirs = ProjectDirs::from("", "",  "tsk-rs").unwrap();
    proj_dirs.config_dir().join("tsk.toml").to_str().unwrap().to_owned()
}

// eof
