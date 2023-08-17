use std::collections::HashMap;

use color_eyre::eyre::{Context, Result};

use crate::{settings::Settings, task::list_tasks};

/// scan all active and done tasks to find tags in use
pub fn scan_tags(settings: &Settings) -> Result<HashMap<String, usize>> {
    let tasks = list_tasks(&None, &true, settings)
        .with_context(|| "while scanning through all tasks")?;

    let mut collected_tags: HashMap<String, usize> = HashMap::new();

    for task in tasks {
        if let Some(tags) = task.tags {
            for tag in tags {
                *collected_tags.entry(tag).or_insert(0) += 1;
            }
        }
    }

    Ok(collected_tags)
}

// eof