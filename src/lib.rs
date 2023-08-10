#![warn(missing_docs)]

//! Task management done right
//!
//! This library provides common interface for tasks, notes and metadata

/// Metadata abstractions
pub mod metadata;

/// Note abstractions
#[cfg(feature = "note")]
pub mod note;

/// Parser for task notation describing the task and its metadata in a single line of text
pub mod parser;

/// Settings parsers
pub mod settings;

/// Task abtsractions
pub mod task;

// eof
