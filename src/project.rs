use std::collections::HashMap;

use color_eyre::eyre::{Context, Result};

use crate::{settings::Settings, task::list_tasks};

/// scan all active and done tasks to find projects in use
pub fn scan_projects(settings: &Settings) -> Result<HashMap<String, usize>> {
    let tasks = list_tasks(&None, &true, settings)
        .with_context(|| "while scanning through all tasks")?;

    let mut collected_projects: HashMap<String, usize> = HashMap::new();

    for task in tasks {
        if let Some(project) = task.project {
                *collected_projects.entry(project).or_insert(0) += 1;
        }
    }

    Ok(collected_projects)
}

// eof