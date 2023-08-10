use bat::{Input, PrettyPrinter};
use color_eyre::eyre::{bail, Context, Result};
use config::Config;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, fs::create_dir_all, path::PathBuf};
use thiserror::Error;

/// Errors that can occur during settings handling
#[derive(Error, Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// Data directory where tasks and notes are stored does not exist
    #[error("data directory does not exist, and createdir is set to false")]
    DataDirectoryDoesNotExist,
}

/// Task spesific settings
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskSettings {
    /// If true special tag "hold" is removed from the task when time tracking is started
    pub autorelease: bool,
    /// If true and special tag "start" is present when creating a new task then time tracking for
    /// the task is immediately started.
    pub starttag: bool,
    /// If true then special tags are listed along custom tags when listing tasks
    pub specialvisible: bool,
    /// If true then when ever task is marked done while running the timetracking (if running) is
    /// automatically stopped.
    pub stopondone: bool,
    /// If true when marking task done the special tags that might be in effect for the task are
    /// also removed.
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

/// Client binary output settings
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputSettings {
    /// Use colored output?
    pub colors: bool,
    /// Use grided output?
    pub grid: bool,
    /// Show line numbers?
    pub numbers: bool,
    /// Display namespace that is active
    pub namespace: bool,
    /// If the description of the task is longer than this then truncate the string for output
    pub descriptionlength: usize,
    /// Calculates totals and display them in task/note listings
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

/// Note spesific settings
#[cfg(feature = "note")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteSettings {
    /// If true then when note is created for an task the description of the Task is set as
    /// Markdown title
    pub description: bool,
    /// If true then when note is created/edited for a task the current local timestamp is added as
    /// subheader to the Markdown
    pub timestamp: bool,
}

#[cfg(feature = "note")]
impl Default for NoteSettings {
    fn default() -> Self {
        Self {
            description: true,
            timestamp: true,
        }
    }
}

/// Settings related to the the data storage path and handling
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DataSettings {
    /// Path under which the task and note files are created. If not spesified system default is
    /// used.
    pub path: String,
    /// If true and the data directory does not exist then the folder is created. If False then
    /// error is thrown if directory does not exist.
    pub createdir: bool,
    /// How many task and note data file backups should be rotated?
    pub rotate: usize,
}

impl Default for DataSettings {
    fn default() -> Self {
        let proj_dirs = ProjectDirs::from("", "", "tsk-rs").unwrap();

        Self {
            path: String::from(proj_dirs.data_dir().to_str().unwrap()),
            createdir: true,
            rotate: 3,
        }
    }
}

/// Client tool settings
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing)]
    /// Namespace is read from environment or from command line. Default namespace is "default" and
    /// cant be changed with configuration. The namespace is populated to the settings struct
    /// during runtime only.
    pub namespace: String,
    /// Settings related to data storage
    pub data: DataSettings,
    #[cfg(feature = "note")]
    /// Settings related to notes only
    pub note: NoteSettings,
    /// Settings related to tasks only
    pub task: TaskSettings,
    /// Display/output settings
    pub output: OutputSettings,
}

impl Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", toml::to_string(&self).unwrap())
    }
}

impl Settings {
    /// Create new settings struct by creating defaults and overwriting them from either config
    /// files or environment variables.
    pub fn new(namespace: Option<String>, config_file: &str) -> Result<Self> {
        let settings: Settings = Config::builder()
            .set_override_option("namespace", namespace)?
            .add_source(config::File::with_name(config_file).required(false))
            .add_source(
                config::Environment::with_prefix("TSK")
                    .try_parsing(true)
                    .separator("_"),
            )
            .build()
            .with_context(|| "while reading configuration")?
            .try_deserialize()
            .with_context(|| "while applying defaults to configuration")?;

        Ok(settings)
    }

    /// Returns the base database path where the task and notes files are stored in their own
    /// subfolders.
    pub fn db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = PathBuf::from(&self.data.path).join(&self.namespace);
        if !pathbuf.is_dir() && self.data.createdir {
            create_dir_all(&pathbuf).with_context(|| "while creating data directory")?;
        } else if !pathbuf.is_dir() && !self.data.createdir {
            bail!(SettingsError::DataDirectoryDoesNotExist);
        }
        Ok(pathbuf)
    }

    /// Return the subpath where task files are stored in under the dbpath
    pub fn task_db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = &self.db_pathbuf()?.join("tasks");
        if !pathbuf.is_dir() && self.data.createdir {
            create_dir_all(&pathbuf).with_context(|| "while creating tasks data directory")?;
        } else if !pathbuf.is_dir() && !self.data.createdir {
            bail!(SettingsError::DataDirectoryDoesNotExist);
        }
        Ok(pathbuf.to_path_buf())
    }

    /// Return the subpath where note files are stored in under the dbpath
    #[cfg(feature = "note")]
    pub fn note_db_pathbuf(&self) -> Result<PathBuf> {
        let pathbuf = &self.db_pathbuf()?.join("notes");
        if !pathbuf.is_dir() && self.data.createdir {
            create_dir_all(&pathbuf).with_context(|| "while creating notes data directory")?;
        } else if !pathbuf.is_dir() && !self.data.createdir {
            bail!(SettingsError::DataDirectoryDoesNotExist);
        }
        Ok(pathbuf.to_path_buf())
    }
}

/// Show active configuration. Uses Bat.
pub fn show_config(settings: &Settings) -> Result<()> {
    let settings_toml = format!("{}", settings);
    PrettyPrinter::new()
        .language("toml")
        .input(Input::from_bytes(settings_toml.as_bytes()))
        .colored_output(settings.output.colors)
        .grid(settings.output.grid)
        .line_numbers(settings.output.numbers)
        .print()
        .with_context(|| "while trying to prettyprint yaml")?;

    Ok(())
}

/// Returns default configuration path if none is configured at env or via the command line
pub fn default_config() -> String {
    let proj_dirs = ProjectDirs::from("", "", "tsk-rs").unwrap();
    proj_dirs
        .config_dir()
        .join("tsk.toml")
        .to_str()
        .unwrap()
        .to_owned()
}

// eof
