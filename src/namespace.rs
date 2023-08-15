use std::fs;

use crate::settings::Settings;
use color_eyre::eyre::{Context, Result};

/// Namespace abstraction and metadata
pub struct Namespace {
    /// If the namespace detected is the one currently active in configuration
    pub is_current: bool,
    /// Name of the detected namespace
    pub name: String,
}

/// List all namespaces available in the ecosystem
pub fn list_namespaces(settings: &Settings) -> Result<Vec<Namespace>> {
    let mut namespaces: Vec<Namespace> = vec![];

    for entry in fs::read_dir(settings.data.path.clone()).with_context(|| "error while scanning database directory for namespaces")? {
        let entry = entry?;
        if entry.metadata()?.is_dir() {
            let name = entry.file_name().to_str().unwrap().to_string(); // TODO: fix unwrap
            let is_current = if name == settings.namespace { true } else { false };
            namespaces.push(Namespace { is_current, name });
        }
    }

    Ok(namespaces)
}

// eof