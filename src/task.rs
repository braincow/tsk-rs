use crate::{
    metadata::MetadataKeyValuePair,
    parser::task_lexicon::{parse_task, Expression},
    settings::Settings,
};
use chrono::{DateTime, Duration, Local, NaiveDateTime};
use color_eyre::eyre::{bail, Context, Result};
use file_lock::{FileLock, FileOptions};
use glob::glob;
use serde::{Deserialize, Serialize};
use simple_file_rotation::FileRotation;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
};
use strum::{EnumString, IntoStaticStr};
use thiserror::Error;
use uuid::Uuid;

#[cfg(feature = "notify")]
use crate::notify::DatabaseFileType;

/// Available priorities for a task
/// Each priority level has an different effect to the overall urgency level calculations
#[derive(EnumString, IntoStaticStr, clap::ValueEnum, Clone, Eq, PartialEq, Debug)]
pub enum TaskPriority {
    /// Low priority
    Low,
    /// Medium priority
    Medium,
    /// High priority
    High,
    /// Critical priority
    Critical,
}

/// Errors that can occure when working with a task and its metadata
#[derive(Error, Debug, PartialEq, Eq)]
pub enum TaskError {
    /// Multiple projects were defined in the task descriptor. Not allowed.
    #[error("only one project identifier allowed")]
    MultipleProjectsNotAllowed,
    /// Multiple priorities were defined in the task description. Not allowed.
    #[error("only one priority identifier allowed")]
    MultiplePrioritiesNotAllowed,
    /// Multiple due dates were defined in the task descriptor. Not allowed.
    #[error("only one due date identifier allowed")]
    MultipleDuedatesNotAllowed,
    /// Multiple metadata pairs with same key was defined in the task descriptor. Not allowed.
    #[error("only one instance of metadata key `{0}` is allowed")]
    IdenticalMetadataKeyNotAllowed(String),
    /// Metadata key defined in the task descriptor is malformed or was not prefixed with "x-".
    #[error("metadata key name invalid `{0}`. try with prefix `x-{0}`")]
    MetadataPrefixInvalid(String),
    /// Task was already marked to be completed and thus it cant be modified.
    #[error("task already completed. cannot modify")]
    TaskAlreadyCompleted,
    /// Task is already running
    #[error("task already running")]
    TaskAlreadyRunning,
    /// Task is not running
    #[error("task not running")]
    TaskNotRunning,
    /// Task descriptor was empty
    #[error("task descriptor cant be an empty string")]
    TaskDescriptorEmpty,
    /// Conversion error from notify event kind. Needs to be Task for Task.
    #[cfg(feature = "notify")]
    #[error("notifier result kind is not for a Task")]
    IncompatibleNotifyKind,
}

/// Time track entry holds information about a span of time while the task was/is being worked on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTrack {
    /// Local timestamp for the moment in time when the time tracking was started
    pub start_time: DateTime<Local>,
    /// Local timestamp for the moment in time when the time tracking ended
    pub end_time: Option<DateTime<Local>>,
    /// Optional annotation or a description for the time span e.g what was done while working on
    /// the task?
    pub annotation: Option<String>,
}

/// Task data abstraction as a Rust struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for the task
    pub id: Uuid,
    /// Description or a title of the task
    pub description: String,
    /// Is the task completed or not
    pub done: bool,
    /// To which project (if any) is this task part of?
    pub project: Option<String>,
    /// Tags (if any) for the task that can be used to group several tasks of some kind
    pub tags: Option<Vec<String>>,
    /// Key, value pairs holding either user added metadata fields prepended with the "x-" string
    /// or task management internal metadata.
    pub metadata: BTreeMap<String, String>,
    /// List of optional [TimeTrack] entries.
    pub timetracker: Option<Vec<TimeTrack>>,
}

impl Task {
    /// Instantiate task by loading it from disk based on notifier event
    #[cfg(feature = "notify")]
    pub fn from_notify_event(event: DatabaseFileType, settings: &Settings) -> Result<Task> {
        match event {
            DatabaseFileType::Task(uuid) => load_task(&uuid.to_string(), settings),
            _ => bail!(TaskError::IncompatibleNotifyKind)
        }
    }

    /// Search the task with a string and try to match it to available information. Return true if
    /// the task matches.
    pub fn loose_match(&self, search: &str) -> bool {
        if self
            .description
            .to_lowercase()
            .contains(&search.to_lowercase())
        {
            return true;
        }

        if let Some(project) = self.project.clone() {
            if project.to_lowercase().contains(&search.to_lowercase()) {
                return true;
            }
        }

        if let Some(tags) = self.tags.clone() {
            for tag in tags {
                if tag.to_lowercase().contains(&search.to_lowercase()) {
                    return true;
                }
            }
        }

        // @TODO: match to metadata keys/values

        false
    }

    /// Returns true if the task is running
    pub fn is_running(&self) -> bool {
        if self.timetracker.is_none() {
            return false;
        }

        for timetrack in self.timetracker.as_ref().unwrap() {
            if timetrack.end_time.is_none() {
                return true;
            }
        }

        false
    }

    /// Returns current [TimeTrack] entry for the task if one is running. Returns None if time
    /// tracking is not active.
    pub fn current_timetrack(&self) -> Option<(usize, TimeTrack)> {
        for (i, timetrack) in self.timetracker.as_ref().unwrap().iter().enumerate() {
            if timetrack.end_time.is_none() {
                return Some((i, timetrack.clone()));
            }
        }
        None
    }

    /// Start time tracking for the task and return the [TimeTrack] entry
    pub fn start(&mut self, annotation: &Option<String>) -> Result<TimeTrack> {
        let tt: TimeTrack;
        if self.done {
            bail!(TaskError::TaskAlreadyCompleted);
        }
        if !self.is_running() {
            let timestamp = chrono::offset::Local::now();
            let mut timetracks: Vec<TimeTrack>;
            if self.timetracker.is_some() {
                timetracks = self.timetracker.as_ref().unwrap().to_vec();
            } else {
                timetracks = vec![];
            }
            tt = TimeTrack {
                start_time: timestamp,
                end_time: None,
                annotation: annotation.clone(),
            };
            timetracks.push(tt.clone());
            self.timetracker = Some(timetracks);
        } else {
            bail!(TaskError::TaskAlreadyRunning);
        }

        Ok(tt)
    }

    /// Stop time tracking for the task. Return the [TimeTrack] entry that was concluded.
    pub fn stop(&mut self) -> Result<Option<TimeTrack>> {
        if self.done {
            bail!(TaskError::TaskAlreadyCompleted);
        }

        let retval: Option<TimeTrack>;

        if self.is_running() {
            let timestamp = chrono::offset::Local::now();
            let (pos, mut timetrack) = self.current_timetrack().unwrap();
            let mut timetracks: Vec<TimeTrack> = self.timetracker.as_ref().unwrap().to_vec();
            timetrack.end_time = Some(timestamp);
            _ = timetracks.remove(pos);
            timetracks.insert(pos, timetrack.clone());
            self.timetracker = Some(timetracks);
            retval = Some(timetrack);
        } else {
            bail!(TaskError::TaskNotRunning);
        }

        Ok(retval)
    }

    /// Return the runtime (delta of start timestamp of [TimeTrack] and current timestamp) of a
    /// running task.
    pub fn current_runtime(&self) -> Option<Duration> {
        if !self.is_running() {
            return None;
        }
        let now = chrono::offset::Local::now();
        let (_, timetrack) = self.current_timetrack().unwrap();
        let runtime = now - timetrack.start_time;

        Some(runtime)
    }

    /// Load task YAML formatted file from the disk
    pub fn load_yaml_file_from(task_pathbuf: &PathBuf) -> Result<Self> {
        let mut file =
            File::open(task_pathbuf).with_context(|| "while opening task yaml file for reading")?;
        let mut task_yaml: String = String::new();
        file.read_to_string(&mut task_yaml)
            .with_context(|| "while reading task yaml file")?;
        Task::from_yaml_string(&task_yaml)
            .with_context(|| "while serializing yaml into task struct")
    }

    /// Save task as YAML formatted file to the disk
    pub fn save_yaml_file_to(&mut self, task_pathbuf: &PathBuf, rotate: &usize) -> Result<()> {
        // rotate existing file with same name if present
        if task_pathbuf.is_file() && rotate > &0 {
            FileRotation::new(&task_pathbuf)
                .max_old_files(*rotate)
                .file_extension("yaml".to_string())
                .rotate()
                .with_context(|| "while rotating task data file backups")?;
        }
        // save file by locking
        let should_we_block = true;
        let options = FileOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .append(false);
        {
            let mut filelock = FileLock::lock(task_pathbuf, should_we_block, options)
                .with_context(|| "while opening new task yaml file")?;
            filelock
                .file
                .write_all(
                    self.to_yaml_string()
                        .with_context(|| "while serializing task struct to yaml")?
                        .as_bytes(),
                )
                .with_context(|| "while writing to task yaml file")?;
            filelock
                .file
                .flush()
                .with_context(|| "while flushing os caches to disk")?;
            filelock
                .file
                .sync_all()
                .with_context(|| "while syncing filesystem metadata")?;
        }

        Ok(())
    }

    /// Mark this task as done
    pub fn mark_as_completed(&mut self) -> Result<()> {
        if self.is_running() {
            // if the task is running stop the current timetrack first to cleanup properly
            self.stop().with_context(|| "while stopping a task")?;
        }
        if !self.done {
            // only mark as done and add metadata if the task is not done yet. this keeps original task-completed-time intact
            self.done = true;
            let timestamp = chrono::offset::Local::now();
            self.metadata.insert(
                String::from("tsk-rs-task-completed-time"),
                timestamp.to_rfc3339(),
            );
        }

        Ok(())
    }

    /// Create a new task with description only
    pub fn new(description: String) -> Result<Self> {
        let timestamp = chrono::offset::Local::now();
        let mut metadata: BTreeMap<String, String> = BTreeMap::new();
        metadata.insert(
            String::from("tsk-rs-task-create-time"),
            timestamp.to_rfc3339(),
        );
        let mut task = Task {
            id: Uuid::new_v4(),
            description,
            done: false,
            project: None,
            tags: None,
            metadata,
            timetracker: None,
        };
        // Calculate the score into metadata
        let score = task.score().with_context(|| "error during task score insert into metadata")?;
        task.metadata.insert("tsk-rs-task-score".to_owned(), format!("{}", score));
        
        Ok(task)
    }

    /// Serialize the task as YAML string
    pub fn to_yaml_string(&mut self) -> Result<String> {
        // Calculate the score into metadata
        let score = self.score().with_context(|| "error during task score refresh into metadata")?;
        self.metadata.insert("tsk-rs-task-score".to_owned(), format!("{}", score));

        serde_yaml::to_string(self).with_context(|| "unable to serialize task struct as yaml")
    }

    /// Deserialize the task from YAML string
    pub fn from_yaml_string(input: &str) -> Result<Self> {
        let mut task: Task = serde_yaml::from_str(input)
            .with_context(|| "unable to deserialize yaml into task struct")?;
        // Recalculate the score into metadata
        let score = task.score().with_context(|| "error during task score refresh into metadata")?;
        task.metadata.insert("tsk-rs-task-score".to_owned(), format!("{}", score));
        Ok(task)
    }

    /// Create a new task from task descriptor string
    ///
    /// Example: `This is a prj:Project task that has to be done. due:2022-08-01T16:00:00 prio:low meta:x-fuu=bar tag:some tag:tags tag:can tag:be tag:added`
    pub fn from_task_descriptor(input: &String) -> Result<Self> {
        if input.is_empty() {
            bail!(TaskError::TaskDescriptorEmpty);
        }
        let expressions =
            parse_task(input.to_string()).with_context(|| "while parsing task descriptor")?;

        let mut description: String = String::new();
        let mut tags: Vec<String> = vec![];
        let mut metadata: BTreeMap<String, String> = BTreeMap::new();
        let mut project: String = String::new();

        for expr in expressions {
            match expr {
                Expression::Description(desc) => {
                    // always extend the existing desctiption text with additional
                    //  text that is found later on
                    if !description.is_empty() {
                        description = format!("{} {}", description, desc);
                    } else {
                        description = desc;
                    }
                }
                Expression::Tag(tag) => {
                    let new_tag = tag;
                    if !tags.contains(&new_tag) {
                        // add the tag only if it is not already added (drop duplicates silently)
                        tags.push(new_tag);
                    }
                }
                Expression::Metadata { key, value } => {
                    let new_key = key.to_ascii_lowercase();
                    if !new_key.starts_with("x-") {
                        bail!(TaskError::MetadataPrefixInvalid(new_key))
                    }
                    if metadata.contains_key(&new_key) {
                        bail!(TaskError::IdenticalMetadataKeyNotAllowed(new_key))
                    }
                    // add metadata key => value pair to map
                    metadata.insert(new_key, value);
                }
                Expression::Project(prj) => {
                    if !project.is_empty() {
                        bail!(TaskError::MultipleProjectsNotAllowed);
                    }
                    // set project
                    project = prj
                }
                Expression::Priority(prio) => {
                    let prio_str: &str = prio.into();
                    let key = "tsk-rs-task-priority".to_string();
                    if metadata.contains_key(&key) {
                        bail!(TaskError::MultiplePrioritiesNotAllowed)
                    }
                    metadata.insert(key, prio_str.to_string());
                }
                Expression::Duedate(datetime) => {
                    let value = datetime.and_local_timezone(Local).unwrap().to_rfc3339();
                    let key = "tsk-rs-task-due-time".to_string();
                    if metadata.contains_key(&key) {
                        bail!(TaskError::MultipleDuedatesNotAllowed)
                    }
                    metadata.insert(key, value);
                }
            };
        }

        let mut ret_tags = None;
        if !tags.is_empty() {
            ret_tags = Some(tags)
        }
        let mut ret_project = None;
        if !project.is_empty() {
            ret_project = Some(project);
        }

        let timestamp = chrono::offset::Local::now();
        metadata.insert(
            String::from("tsk-rs-task-create-time"),
            timestamp.to_rfc3339(),
        );

        let mut task = Task {
            id: Uuid::new_v4(),
            description,
            done: false,
            tags: ret_tags,
            metadata,
            project: ret_project,
            timetracker: None,
        };

        // Calculate the score into metadata
        let score = task.score().with_context(|| "error during task score insert into metadata")?;
        task.metadata.insert("tsk-rs-task-score".to_owned(), format!("{}", score));

        Ok(task)
    }

    /// Calculate the score for the task than can be used to compare urgencies of seperate tasks
    /// and giving a priority.
    fn score(&self) -> Result<usize> {
        // the more "fleshed out" the task is the more higher score it should get
        let mut score: usize = 0;

        if self.project.is_some() {
            // project is valued at 3 points
            score += 3;
        }

        if self.tags.is_some() {
            // each hashtag is valued at two (2) points
            score += self.tags.as_ref().unwrap().len() * 2;
        }

        if self.is_running() {
            // if task is running it gains 15 points
            score += 15;
        }

        if self.timetracker.is_some() {
            // each timetracker entry grants 1 point
            score += self.timetracker.as_ref().unwrap().len();
        }

        if let Some(priority) = self.metadata.get("tsk-rs-task-priority") {
            // priorities have different weights in the score
            match TaskPriority::from_str(priority)
                .with_context(|| "while converting task priority to enum")?
            {
                TaskPriority::Low => score += 1,
                TaskPriority::Medium => score += 3,
                TaskPriority::High => score += 8,
                TaskPriority::Critical => score += 13,
            }
        }

        let timestamp = chrono::offset::Local::now();

        if let Some(duedate_str) = self.metadata.get("tsk-rs-task-due-time") {
            // if due date is present then WHEN has a different score
            let duedate = DateTime::from_str(duedate_str)
                .with_context(|| "while parsing due date string as a datetime")?;
            let diff = duedate - timestamp;

            match diff.num_days() {
                n if n < 0 => score += 10,
                0..=2 => score += 7,
                3..=5 => score += 3,
                _ => score += 1,
            };
        }

        let create_date = DateTime::from_str(self.metadata.get("tsk-rs-task-create-time").unwrap())
            .with_context(|| "while reading task creation date")?;
        let create_diff = timestamp - create_date;
        // as the task gets older each day gives 0.14285715 worth of weight to score. this is rounded when
        //  returned as usize, but this means that every seven days grants one point
        score += (create_diff.num_days() as f32 * 0.142_857_15) as usize;

        // special tags (applied last) reduce the or add to the score
        if let Some(tags) = &self.tags {
            if tags.contains(&"next".to_string()) {
                // just like in taskwarrior special tag "next" gives a huge boost
                score += 100;
            }

            if tags.contains(&"hold".to_string()) {
                // hold will reduce score
                if score >= 20 {
                    score -= 20;
                } else {
                    score = 0;
                }
            }
        }

        Ok(score)
    }

    /// Remove task characteristics
    pub fn unset_characteristic(
        &mut self,
        priority: &bool,
        due_date: &bool,
        tags: &Option<Vec<String>>,
        project: &bool,
        metadata: &Option<Vec<String>>,
    ) -> bool {
        let mut modified = false;

        if *priority {
            let old_prio = self.metadata.remove("tsk-rs-task-priority");
            if old_prio.is_some() {
                modified = true;
            }
        }

        if *due_date {
            let old_duedate = self.metadata.remove("tsk-rs-task-due-time");
            if old_duedate.is_some() {
                modified = true;
            }
        }

        if let Some(tags) = tags {
            let mut task_tags = if let Some(task_tags) = self.tags.clone() {
                task_tags
            } else {
                vec![]
            };

            let mut tags_modified = false;
            for remove_tag in tags {
                if let Some(index) = task_tags.iter().position(|r| r == remove_tag) {
                    task_tags.swap_remove(index);
                    tags_modified = true;
                }
            }

            if tags_modified {
                self.tags = Some(task_tags);
                modified = true;
            }
        }

        if *project {
            self.project = None;
            modified = true;
        }

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

    /// Set task characteristics
    pub fn set_characteristic(
        &mut self,
        priority: &Option<TaskPriority>,
        due_date: &Option<NaiveDateTime>,
        tags: &Option<Vec<String>>,
        project: &Option<String>,
        metadata: &Option<Vec<MetadataKeyValuePair>>,
    ) -> bool {
        let mut modified = false;

        if let Some(priority) = priority {
            let prio_str: &str = priority.into();
            self.metadata
                .insert("tsk-rs-task-priority".to_string(), prio_str.to_string());

            modified = true;
        }

        if let Some(due_date) = due_date {
            self.metadata.insert(
                "tsk-rs-task-due-time".to_string(),
                due_date.and_local_timezone(Local).unwrap().to_rfc3339(),
            );
            modified = true;
        }

        if let Some(tags) = tags {
            let mut task_tags = if let Some(task_tags) = self.tags.clone() {
                task_tags
            } else {
                vec![]
            };

            let mut tags_modified = false;
            for new_tag in tags {
                if !task_tags.contains(new_tag) {
                    task_tags.push(new_tag.to_string());
                    tags_modified = true;
                }
            }

            if tags_modified {
                self.tags = Some(task_tags);
                modified = true;
            }
        }

        if project.is_some() {
            self.project = project.clone();
            modified = true;
        }

        if let Some(metadata) = metadata {
            for new_metadata in metadata {
                self.metadata
                    .insert(new_metadata.key.clone(), new_metadata.value.clone());
                modified = true;
            }
        }

        modified
    }
}

/// Construct a taskbuf that points to YAML file on disk where filename is the id
pub fn task_pathbuf_from_id(id: &String, settings: &Settings) -> Result<PathBuf> {
    Ok(settings
        .task_db_pathbuf()?
        .join(PathBuf::from(format!("{}.yaml", id))))
}

/// Construct a taskbuf that points to YAML file on disk where the id is pulled from [Task]
/// metadata
pub fn task_pathbuf_from_task(task: &Task, settings: &Settings) -> Result<PathBuf> {
    task_pathbuf_from_id(&task.id.to_string(), settings)
}

/// Load task from file, identified by id
pub fn load_task(id: &String, settings: &Settings) -> Result<Task> {
    let task_pathbuf =
        task_pathbuf_from_id(id, settings).with_context(|| "while building path of the file")?;
    let task = Task::load_yaml_file_from(&task_pathbuf)
        .with_context(|| "while loading task yaml file for editing")?;
    Ok(task)
}

/// Save task to disk, identified by the id in its metadata
pub fn save_task(task: &mut Task, settings: &Settings) -> Result<()> {
    let task_pathbuf = task_pathbuf_from_task(task, settings)?;
    task.save_yaml_file_to(&task_pathbuf, &settings.data.rotate)
        .with_context(|| "while saving task yaml file")?;
    Ok(())
}

/// Create a new task
pub fn new_task(descriptor: String, settings: &Settings) -> Result<Task> {
    let mut task =
        Task::from_task_descriptor(&descriptor).with_context(|| "while parsing task descriptor")?;

    // once the task file has been created check for special tags that should take immediate action
    if let Some(tags) = task.tags.clone() {
        if tags.contains(&"start".to_string()) && settings.task.starttag {
            start_task(
                &task.id.to_string(),
                &Some("started on creation".to_string()),
                settings,
            )?;
        }
    }

    save_task(&mut task, settings).with_context(|| "while saving new task")?;
    Ok(task)
}

/// Start tracking the task, load & save the file on disk
pub fn start_task(id: &String, annotation: &Option<String>, settings: &Settings) -> Result<Task> {
    let mut task = load_task(id, settings)?;
    task.start(annotation)
        .with_context(|| "while starting time tracking")?;

    // if special tag (hold) is present then release the hold by modifying tags.
    if settings.task.autorelease {
        task.unset_characteristic(
            &false,
            &false,
            &Some(vec!["hold".to_string()]),
            &false,
            &None,
        );
    }

    save_task(&mut task, settings).with_context(|| "while saving started task")?;
    Ok(task)
}

/// Stop tracking the task, load & save the file on disk
pub fn stop_task(id: &String, done: &bool, settings: &Settings) -> Result<Task> {
    let mut task = load_task(id, settings)?;
    task.stop()
        .with_context(|| "while stopping time tracking")?;

    if *done {
        complete_task(&mut task, settings)?;
    }

    save_task(&mut task, settings).with_context(|| "while saving stopped task")?;

    Ok(task)
}

/// Mark the task completed, load & save the file on disk
pub fn complete_task(task: &mut Task, settings: &Settings) -> Result<()> {
    if task.is_running() && settings.task.stopondone {
        // task is running, so first stop it
        stop_task(&task.id.to_string(), &false, settings)?;
    }

    // remove special tags when task is marked completed
    if settings.task.clearpsecialtags {
        task.unset_characteristic(
            &false,
            &false,
            &Some(vec![
                "start".to_string(),
                "next".to_string(),
                "hold".to_string(),
            ]),
            &false,
            &None,
        );
    }

    task.mark_as_completed()
        .with_context(|| "while completing task")?;
    save_task(task, settings)?;

    Ok(())
}

/// Load all tasks and return the sum of tasks.
pub fn amount_of_tasks(settings: &Settings, include_backups: bool) -> Result<usize> {
    let mut tasks: usize = 0;
    let task_pathbuf: PathBuf = task_pathbuf_from_id(&"*".to_string(), settings)?;
    for task_filename in glob(task_pathbuf.to_str().unwrap())
        .with_context(|| "while traversing task data directory files")?
    {
        // if the filename is u-u-i-d.3.yaml for example it is a backup file and should be disregarded
        if task_filename
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .collect::<Vec<_>>()[1]
            != "yaml"
            && !include_backups
        {
            continue;
        }
        tasks += 1;
    }
    Ok(tasks)
}

/// List all tasks that match an optional search criteria
pub fn list_tasks(
    search: &Option<String>,
    include_done: &bool,
    settings: &Settings,
) -> Result<Vec<Task>> {
    let task_pathbuf: PathBuf = task_pathbuf_from_id(&"*".to_string(), settings)?;

    let mut found_tasks: Vec<Task> = vec![];
    for task_filename in glob(task_pathbuf.to_str().unwrap())
        .with_context(|| "while traversing task data directory files")?
    {
        // if the filename is u-u-i-d.3.yaml for example it is a backup file and should be disregarded
        if task_filename
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .collect::<Vec<_>>()[1]
            != "yaml"
        {
            continue;
        }

        let task = Task::load_yaml_file_from(&task_filename?)
            .with_context(|| "while loading task from yaml file")?;

        if !task.done || *include_done {
            if let Some(search) = search {
                if task.loose_match(search) {
                    // a part of key information matches search term, so the task is included
                    found_tasks.push(task);
                }
            } else {
                // search term is empty so everything matches
                found_tasks.push(task);
            }
        }
    }
    found_tasks.sort_by_key(|k| k.score().unwrap());
    found_tasks.reverse();

    Ok(found_tasks)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Datelike};

    use super::*;

    static FULLTESTCASEINPUT: &str = "some task description here @project-here #taghere #a-second-tag %x-meta=data %x-fuu=bar additional text at the end";
    static FULLTESTCASEINPUT2: &str = "some task description here PRJ:project-here #taghere TAG:a-second-tag META:x-meta=data %x-fuu=bar DUE:2022-08-16T16:56:00 PRIO:medium and some text at the end";
    static NOEXPRESSIONSINPUT: &str = "some task description here without expressions";
    static MULTIPROJECTINPUT: &str = "this has a @project-name, and a @second-project name";
    static DUPLICATEMETADATAINPUT: &str = "this has %x-fuu=bar definied again with %x-fuu=bar";
    static INVALIDMETADATAKEY: &str = "here is an %invalid=metadata key";
    static YAMLTESTINPUT: &str = "id: bd6f75aa-8c8d-47fb-b905-d9f7b15c782d\ndescription: some task description here additional text at the end\ndone: false\nproject: project-here\ntags:\n- taghere\n- a-second-tag\nmetadata:\n  x-meta: data\n  x-fuu: bar\n  x-meta: data\n  tsk-rs-task-create-time: 2022-08-06T07:55:26.568460389+00:00\n";

    #[test]
    fn test_from_yaml() {
        let task = Task::from_yaml_string(YAMLTESTINPUT).unwrap();

        assert_eq!(task.project, Some(String::from("project-here")));
        assert_eq!(
            task.description,
            "some task description here additional text at the end"
        );
        assert_eq!(
            task.tags,
            Some(vec![String::from("taghere"), String::from("a-second-tag")])
        );
        assert_eq!(task.metadata.get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.get("x-fuu"), Some(&String::from("bar")));

        let timestamp =
            DateTime::parse_from_rfc3339(task.metadata.get("tsk-rs-task-create-time").unwrap())
                .unwrap();
        assert_eq!(timestamp.year(), 2022);
        assert_eq!(timestamp.month(), 8);
        assert_eq!(timestamp.day(), 6);
    }

    #[test]
    fn test_to_yaml() {
        let mut task = Task::from_task_descriptor(&FULLTESTCASEINPUT.to_string()).unwrap();

        // for testing we need to know the UUID so create a new one and override autoassigned one
        let test_uuid = Uuid::parse_str("bd6f75aa-8c8d-47fb-b905-d9f7b15c782d").unwrap();
        task.id = test_uuid;

        let yaml_string = task.to_yaml_string().unwrap();
        assert_eq!(yaml_string,
            format!("id: {}\ndescription: {}\ndone: false\nproject: {}\ntags:\n- {}\n- {}\nmetadata:\n  tsk-rs-task-create-time: {}\n  tsk-rs-task-score: '7'\n  x-fuu: {}\n  x-meta: {}\ntimetracker: null\n",
                task.id,
                task.description,
                task.project.unwrap(),
                task.tags.clone().unwrap().get(0).unwrap(),
                task.tags.clone().unwrap().get(1).unwrap(),
                task.metadata.clone().get("tsk-rs-task-create-time").unwrap(),
                task.metadata.clone().get("x-fuu").unwrap(),
                task.metadata.clone().get("x-meta").unwrap(),
            ));
    }

    #[test]
    fn parse_full_testcase() {
        let task = Task::from_task_descriptor(&FULLTESTCASEINPUT.to_string()).unwrap();

        assert_eq!(task.project, Some(String::from("project-here")));
        assert_eq!(
            task.description,
            "some task description here additional text at the end"
        );
        assert_eq!(
            task.tags,
            Some(vec![String::from("taghere"), String::from("a-second-tag")])
        );
        assert_eq!(task.metadata.get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.get("x-fuu"), Some(&String::from("bar")));
    }

    #[test]
    fn parse_full_testcase2() {
        let task = Task::from_task_descriptor(&FULLTESTCASEINPUT2.to_string()).unwrap();

        assert_eq!(task.project, Some(String::from("project-here")));
        assert_eq!(
            task.description,
            "some task description here and some text at the end"
        );
        assert_eq!(
            task.tags,
            Some(vec![String::from("taghere"), String::from("a-second-tag")])
        );
        assert_eq!(task.metadata.get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.get("x-fuu"), Some(&String::from("bar")));
        assert_eq!(
            task.metadata.get("tsk-rs-task-priority"),
            Some(&String::from("Medium"))
        );
        //assert_eq!(task.metadata.get("tsk-rs-task-due-time"), );
    }

    #[test]
    fn parse_no_expressions() {
        let task = Task::from_task_descriptor(&NOEXPRESSIONSINPUT.to_string()).unwrap();

        assert_eq!(task.project, None);
        assert_eq!(task.description, NOEXPRESSIONSINPUT);
        assert_eq!(task.tags, None);

        assert!(task.metadata.get("tsk-rs-task-create-time").is_some());
    }

    #[test]
    fn reject_multiple_projects() {
        let task = Task::from_task_descriptor(&MULTIPROJECTINPUT.to_string());

        assert_eq!(
            task.unwrap_err().downcast::<TaskError>().unwrap(),
            TaskError::MultipleProjectsNotAllowed
        );
    }

    #[test]
    fn reject_duplicate_metadata() {
        let task = Task::from_task_descriptor(&DUPLICATEMETADATAINPUT.to_string());

        assert_eq!(
            task.unwrap_err().downcast::<TaskError>().unwrap(),
            TaskError::IdenticalMetadataKeyNotAllowed(String::from("x-fuu"))
        );
    }

    #[test]
    fn require_metadata_prefix() {
        let task = Task::from_task_descriptor(&INVALIDMETADATAKEY.to_string());

        assert_eq!(
            task.unwrap_err().downcast::<TaskError>().unwrap(),
            TaskError::MetadataPrefixInvalid(String::from("invalid"))
        );
    }
}

// eof
