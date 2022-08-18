use crate::parser::task_lexicon::{Expression, parse_task};
use std::{collections::BTreeMap, path::PathBuf, io::{Write, Read}, fs::File, fmt::Display, str::FromStr};
use chrono::{DateTime, Duration, Local};
use file_lock::{FileLock, FileOptions};
use serde::{Serialize, Deserialize};
use simple_file_rotation::FileRotation;
use strum::{IntoStaticStr, EnumString};
use thiserror::Error;
use anyhow::{bail, Result, Context};
use uuid::Uuid;

#[derive(EnumString, IntoStaticStr, clap::ValueEnum, Clone, Eq, PartialEq, Debug)]
pub enum TaskPriority {
   Low,
   Medium,
   High,
   Critical,
}

#[derive(Error, Debug, PartialEq)]
pub enum TaskError {
    #[error("only one project identifier allowed")]
    MultipleProjectsNotAllowed,
    #[error("only one priority identifier allowed")]
    MultiplePrioritiesNotAllowed,
    #[error("only one due date identifier allowed")]
    MultipleDuedatesNotAllowed,
    #[error("only one instance of metadata key `{0}` is allowed")]
    IdenticalMetadataKeyNotAllowed(String),
    #[error("metadata key name invalid `{0}`. try with prefix `x-{0}`")]
    MetadataPrefixInvalid(String),
    #[error("task already completed. cannot modify")]
    TaskAlreadyCompleted,
    #[error("task already running")]
    TaskAlreadyRunning,
    #[error("task not running")]
    TaskNotRunning,
    #[error("task descriptor cant be an empty string")]
    TaskDescriptorEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTrack {
    pub start_time: DateTime<Local>,
    pub end_time: Option<DateTime<Local>>,
    pub annotation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub done: bool,
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: BTreeMap<String, String>,
    pub timetracker: Option<Vec<TimeTrack>>,
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_yaml_string().unwrap())
    }
}

impl Task {
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

    pub fn current_timetrack(&self) -> Option<(usize, TimeTrack)> {
        for (i, timetrack) in self.timetracker.as_ref().unwrap().iter().enumerate() {
            if timetrack.end_time.is_none() {
                return Some((i, timetrack.clone()));
            }
        }
        None
    }

    pub fn start(&mut self, annotation: &Option<String>) -> Result<()> {
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
            timetracks.push(TimeTrack { start_time: timestamp, end_time: None, annotation: annotation.clone() });
            self.timetracker = Some(timetracks);
        } else {
            bail!(TaskError::TaskAlreadyRunning);
        }

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if self.done {
            bail!(TaskError::TaskAlreadyCompleted);
        }

        if self.is_running() {
            let timestamp = chrono::offset::Local::now();
            let (pos, mut timetrack) = self.current_timetrack().unwrap();
            let mut timetracks: Vec<TimeTrack> = self.timetracker.as_ref().unwrap().to_vec();
            timetrack.end_time = Some(timestamp);
            _ = timetracks.remove(pos);
            timetracks.insert(pos, timetrack);
            self.timetracker = Some(timetracks);
        } else {
            bail!(TaskError::TaskNotRunning);
        }

        Ok(())
    }

    pub fn current_runtime(&self) -> Option<Duration> {
        if !self.is_running() {
            return None;
        }
        let now = chrono::offset::Local::now();
        let (_, timetrack) = self.current_timetrack().unwrap();
        let runtime = now - timetrack.start_time;

        Some(runtime)
    }

    pub fn load_yaml_file_from(task_pathbuf: &PathBuf) -> Result<Self> {
        let mut file = File::open(task_pathbuf)
            .with_context(|| {"while opening task yaml file for reading"})?;
        let mut task_yaml: String = String::new();
        file.read_to_string(&mut task_yaml)
            .with_context(|| {"while reading task yaml file"})?;
        Task::from_yaml_string(&task_yaml)
            .with_context(|| {"while serializing yaml into task struct"})
    }

    pub fn save_yaml_file_to(&mut self, task_pathbuf: &PathBuf, rotate: &usize ) -> Result<()> {
        // rotate existing file with same name if present
        if task_pathbuf.is_file() && rotate > &0 {
            FileRotation::new(&task_pathbuf).max_old_files(*rotate).file_extension("yaml".to_string()).rotate()
                .with_context(|| {"while rotating task data file backups"})?;
        }
        // save file by locking
        let should_we_block  = true;
        let options = FileOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .append(false);
        {
            let mut filelock= FileLock::lock(task_pathbuf, should_we_block, options)
                .with_context(|| {"while opening new task yaml file"})?;
            filelock.file.write_all(self.to_yaml_string()
                .with_context(|| {"while serializing task struct to yaml"})?.as_bytes())
                    .with_context(|| {"while writing to task yaml file"})?;
            filelock.file.flush().with_context(|| {"while flushing os caches to disk"})?;
            filelock.file.sync_all().with_context(|| {"while syncing filesystem metadata"})?;
        }

        Ok(())
    }

    pub fn mark_as_completed(&mut self) -> Result<()> {
        if self.is_running() {
            // if the task is running stop the current timetrack first to cleanup properly
            self.stop().with_context(|| {"while stopping a task"})?;
        }
        if !self.done {
            // only mark as done and add metadata if the task is not done yet. this keeps original task-completed-time intact
            self.done = true;
            let timestamp = chrono::offset::Local::now();
            self.metadata.insert(String::from("tsk-rs-task-completed-time"), timestamp.to_rfc3339());    
        }

        Ok(())
    }

    pub fn new(description: String) -> Self {
        let timestamp = chrono::offset::Local::now();
        let mut metadata: BTreeMap<String, String> = BTreeMap::new();
        metadata.insert(String::from("tsk-rs-task-create-time"), timestamp.to_rfc3339());
        Self { id: Uuid::new_v4(), description, done: false, project: None, tags: None, metadata, timetracker: None }
    }

    pub fn to_yaml_string(&self) -> Result<String> {       
        serde_yaml::to_string(self).with_context(|| {"unable to serialize task struct as yaml"})
    }

    pub fn from_yaml_string(input: &str) -> Result<Self> {
        let task: Task = serde_yaml::from_str(input).with_context(|| {"unable to deserialize yaml into task struct"})?;
        Ok(task)
    }

    pub fn from_task_descriptor(input: &String) -> Result<Self> {
        if input.is_empty() {
            bail!(TaskError::TaskDescriptorEmpty);
        }
        let expressions = parse_task(input.to_string()).with_context(|| {"while parsing task descriptor"})?;

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
                },
                Expression::Tag(tag) => {
                    let new_tag = tag;
                    if !tags.contains(&new_tag) {
                        // add the tag only if it is not already added (drop duplicates silently)
                        tags.push(new_tag);
                    }
                },
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
                },
                Expression::Project(prj) => {
                    if !project.is_empty() {
                        bail!(TaskError::MultipleProjectsNotAllowed);
                    }
                    // set project
                    project = prj
                },
                Expression::Priority(prio) => {
                    let prio_str: &str = prio.into();
                    let key = "tsk-rs-task-priority".to_string();
                    if metadata.contains_key(&key) {
                        bail!(TaskError::MultiplePrioritiesNotAllowed)
                    }
                    metadata.insert(key, prio_str.to_string());
                },
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
        metadata.insert(String::from("tsk-rs-task-create-time"), timestamp.to_rfc3339());

        Ok(Self {
            id: Uuid::new_v4(),
            description,
            done: false,
            tags: ret_tags,
            metadata,
            project: ret_project,
            timetracker: None,
        })
    }

    pub fn score(&self) -> Result<usize> {
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
            // if task is running it gains 5 points
            score += 5;
        }
    
        if self.timetracker.is_some() {
            // each timetracker entry grants 1 point
            score += self.timetracker.as_ref().unwrap().len();
        }
    
        if let Some(priority) = self.metadata.get("tsk-rs-task-priority") {
            // priorities have different weights in the score
            match TaskPriority::from_str(priority).with_context(|| {"while converting task priority to enum"})? {
                TaskPriority::Low => score += 1,
                TaskPriority::Medium => score += 3,
                TaskPriority::High => score += 8,
                TaskPriority::Critical => score += 13,
            }
        }
    
        let timestamp = chrono::offset::Local::now();

        if let Some(duedate_str) = self.metadata.get("tsk-rs-task-due-time") {
            // if due date is present then WHEN has a different score
            let duedate = DateTime::from_str(duedate_str).with_context(|| {"while parsing due date string as a datetime"})?;
            let diff = duedate - timestamp;

            match diff.num_days() {
                n if n < 0 => score += 10,
                0..=2 => score += 7,
                3..=5 => score += 3,
                _ => score += 1,
            };
        }

        let create_date = DateTime::from_str(self.metadata.get("tsk-rs-task-create-time").unwrap()).with_context(|| {"while reading task creation date"})?;
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
                if score >= 20  {
                    score -= 20;
                } else {
                    score = 0;
                }
            }
        }

        Ok(score)
    }

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
        assert_eq!(task.description, "some task description here additional text at the end");
        assert_eq!(task.tags, Some(vec![String::from("taghere"), String::from("a-second-tag")]));
        assert_eq!(task.metadata.get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.get("x-fuu"), Some(&String::from("bar")));

        let timestamp = DateTime::parse_from_rfc3339(task.metadata.get("tsk-rs-task-create-time").unwrap()).unwrap();
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
            format!("id: {}\ndescription: {}\ndone: false\nproject: {}\ntags:\n- {}\n- {}\nmetadata:\n  tsk-rs-task-create-time: {}\n  x-fuu: {}\n  x-meta: {}\ntimetracker: null\n",
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
        assert_eq!(task.description, "some task description here additional text at the end");
        assert_eq!(task.tags, Some(vec![String::from("taghere"), String::from("a-second-tag")]));
        assert_eq!(task.metadata.get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.get("x-fuu"), Some(&String::from("bar")));
    }

    #[test]
    fn parse_full_testcase2() {
        let task = Task::from_task_descriptor(&FULLTESTCASEINPUT2.to_string()).unwrap();

        assert_eq!(task.project, Some(String::from("project-here")));
        assert_eq!(task.description, "some task description here and some text at the end");
        assert_eq!(task.tags, Some(vec![String::from("taghere"), String::from("a-second-tag")]));
        assert_eq!(task.metadata.get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.get("x-fuu"), Some(&String::from("bar")));
        assert_eq!(task.metadata.get("tsk-rs-task-priority"), Some(&String::from("Medium")));
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

        assert_eq!(task.unwrap_err().downcast::<TaskError>().unwrap(), TaskError::MultipleProjectsNotAllowed);
    }

    #[test]
    fn reject_duplicate_metadata() {
        let task = Task::from_task_descriptor(&DUPLICATEMETADATAINPUT.to_string());

        assert_eq!(task.unwrap_err().downcast::<TaskError>().unwrap(), TaskError::IdenticalMetadataKeyNotAllowed(String::from("x-fuu")));
    }

    #[test]
    fn require_metadata_prefix() {
        let task = Task::from_task_descriptor(&INVALIDMETADATAKEY.to_string());

        assert_eq!(task.unwrap_err().downcast::<TaskError>().unwrap(), TaskError::MetadataPrefixInvalid(String::from("invalid")));
    }
}

// eof