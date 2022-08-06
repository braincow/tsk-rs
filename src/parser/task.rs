use super::lexicon::{Expression, parse};

use std::collections::HashMap;

use serde::{Serialize, Deserialize};
use thiserror::Error;
use anyhow::{bail, Result};
use uuid::Uuid;

#[derive(Error, Debug, PartialEq)]
pub enum TaskError {
    #[error("only one project identifier allowed")]
    MultipleProjectsNotAllowed,
    #[error("only one instance of metadata key `{0}` is allowed")]
    IdenticalMetadataKeyNotAllowed(String),
    #[error("metadata key name invalid `{0}`. try with prefix `x-{0}`")]
    MetadataPrefixInvalid(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, String>>,
}

impl Task {
    pub fn to_yaml_string(&self) -> Result<String> {       
        let yaml = serde_yaml::to_string(self)?;
        Ok(yaml)
    }

    pub fn from_yaml_string(input: &str) -> Result<Self> {
        let task: Task = serde_yaml::from_str(input)?;
        return Ok(task)
    }

    pub fn parse_task_descriptor(input: &'static str) -> Result<Self> {
        let (_, expressions) = parse(input)?;

        let mut description: String = String::new();
        let mut tags: Vec<String> = vec![];
        let mut metadata: HashMap<String, String> = HashMap::new();
        let mut project: String = String::new();

        for expr in expressions {
            match expr {
                Expression::Description(desc) => {
                    // always extend the existing desctiption text with additional
                    //  text that is found later on
                    if !description.is_empty() {
                        description = format!("{} {}", description, desc);
                    } else {
                        description = String::from(desc);
                    }
                },
                Expression::Hashtag(tag) => {
                    let new_tag = String::from(tag);
                    if !tags.contains(&new_tag) {
                        // add the tag only if it is not already added (drop duplicates silently)
                        tags.push(String::from(new_tag));
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
                    metadata.insert(String::from(new_key), String::from(value));
                },
                Expression::Project(prj) => {
                    if !project.is_empty() {
                        bail!(TaskError::MultipleProjectsNotAllowed);
                    }
                    // set project
                    project = String::from(prj)
                }
            };
        }

        // restruct tags and metadata into options for the constructor
        let mut ret_tags = None;
        if tags.len() > 0 {
            ret_tags = Some(tags)
        }
        let mut ret_metadata = None;
        if metadata.len() > 0 {
            ret_metadata = Some(metadata)
        }
        let mut ret_project = None;
        if !project.is_empty() {
            ret_project = Some(project);
        }

        Ok(Self {
            id: Uuid::new_v4(),
            description: description,
            tags: ret_tags,
            metadata: ret_metadata,
            project: ret_project,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static FULLTESTCASEINPUT: &str = "some task description here @project-here #taghere #a-second-tag %x-meta:data %x-fuu:bar additional text at the end";
    static NOEXPRESSIONSINPUT: &str = "some task description here without expressions";
    static MULTIPROJECTINPUT: &str = "this has a @project-name, and a @second-project name";
    static DUPLICATEMETADATAINPUT: &str = "this has %x-fuu:bar definied again with %x-fuu:bar";
    static INVALIDMETADATAKEY: &str = "here is an %invalid:metadata key";
    static YAMLTESTINPUT: &str = "id: bd6f75aa-8c8d-47fb-b905-d9f7b15c782d\ndescription: some task description here additional text at the end\nproject: project-here\ntags:\n- taghere\n- a-second-tag\nmetadata:\n  x-meta: data\n  x-fuu: bar\n";

    #[test]
    fn test_from_yaml() {
        let task = Task::from_yaml_string(YAMLTESTINPUT).unwrap();

        assert_eq!(task.project, Some(String::from("project-here")));
        assert_eq!(task.description, "some task description here additional text at the end");
        assert_eq!(task.tags, Some(vec![String::from("taghere"), String::from("a-second-tag")]));
        assert_eq!(task.metadata.clone().unwrap().get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.clone().unwrap().get("x-fuu"), Some(&String::from("bar")));
    }

    #[test]
    fn test_to_yaml() {
        let mut task = Task::parse_task_descriptor(FULLTESTCASEINPUT).unwrap();

        // for testing we need to know the UUID so create a new one and override autoassigned one
        let test_uuid = Uuid::parse_str("bd6f75aa-8c8d-47fb-b905-d9f7b15c782d").unwrap();
        task.id = test_uuid;

        let yaml_string = task.to_yaml_string().unwrap();
        assert_eq!(yaml_string,
            format!("id: {}\ndescription: {}\nproject: {}\ntags:\n- {}\n- {}\nmetadata:\n  x-fuu: {}\n  x-meta: {}\n",
                task.id,
                task.description,
                task.project.unwrap(),
                task.tags.clone().unwrap().get(0).unwrap(),
                task.tags.clone().unwrap().get(1).unwrap(),
                task.metadata.clone().unwrap().get("x-fuu").unwrap(),
                task.metadata.clone().unwrap().get("x-meta").unwrap(),
            ));
    }

    #[test]
    fn parse_full_testcase() {
        let task = Task::parse_task_descriptor(FULLTESTCASEINPUT).unwrap();

        assert_eq!(task.project, Some(String::from("project-here")));
        assert_eq!(task.description, "some task description here additional text at the end");
        assert_eq!(task.tags, Some(vec![String::from("taghere"), String::from("a-second-tag")]));
        assert_eq!(task.metadata.clone().unwrap().get("x-meta"), Some(&String::from("data")));
        assert_eq!(task.metadata.clone().unwrap().get("x-fuu"), Some(&String::from("bar")));
    }

    #[test]
    fn parse_no_expressions() {
        let task = Task::parse_task_descriptor(NOEXPRESSIONSINPUT).unwrap();

        assert_eq!(task.project, None);
        assert_eq!(task.description, NOEXPRESSIONSINPUT);
        assert_eq!(task.tags, None);
        assert_eq!(task.metadata, None);
    }

    #[test]
    fn reject_multiple_projects() {
        let task = Task::parse_task_descriptor(MULTIPROJECTINPUT);

        assert_eq!(task.unwrap_err().downcast::<TaskError>().unwrap(), TaskError::MultipleProjectsNotAllowed);
    }

    #[test]
    fn reject_duplicate_metadata() {
        let task = Task::parse_task_descriptor(DUPLICATEMETADATAINPUT);

        assert_eq!(task.unwrap_err().downcast::<TaskError>().unwrap(), TaskError::IdenticalMetadataKeyNotAllowed(String::from("x-fuu")));
    }

    #[test]
    fn require_metadata_prefix() {
        let task = Task::parse_task_descriptor(INVALIDMETADATAKEY);

        assert_eq!(task.unwrap_err().downcast::<TaskError>().unwrap(), TaskError::MetadataPrefixInvalid(String::from("invalid")));
    }
}

// eof