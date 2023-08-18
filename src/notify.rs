use notify::{RecursiveMode, recommended_watcher, Watcher};
use std::sync::mpsc;
use std::thread;
use crate::settings::Settings;
use regex::Regex;

/// Which type of a file was modified
#[derive(Debug)]
pub enum DatabaseFileType {
    /// Modified file was a Task file. Enum contains filename and path.
    Task(String),
    /// Modified file was a Note file. Enum contains filename and path.
    Note(String)
}

/// Function type for change callback.
pub type ChangeCallback = fn(DatabaseFileType);

/// Function type for error callback.
pub type ErrorCallback = fn(String);

/// Filesystem monitor
pub struct FilesystemMonitor {
    watcher_thread: Option<thread::JoinHandle<()>>,
}

impl FilesystemMonitor {
    /// Create a new Filesystem monitor
    pub fn new() -> Self {
        FilesystemMonitor { watcher_thread: None }
    }

    /// Watch the database path for changes
    pub fn watch<S: AsRef<Settings>>(
        &mut self,
        settings: S,
        on_change: ChangeCallback,
        on_error: ErrorCallback,
    ) {
        let re = Regex::new(r"^[0-9a-fA-F]{8}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{4}\b-[0-9a-fA-F]{12}\.yaml$").unwrap(); // TODO: fix unwrap
        let path = settings.as_ref().db_pathbuf().unwrap(); // TODO: fix unwrap
        let task_db_path = settings.as_ref().task_db_pathbuf().unwrap(); // TODO: fix unwrap
        let task_db_path_str = task_db_path.to_str().unwrap().to_string();  // TODO: fix unwrap
        let note_db_path = settings.as_ref().note_db_pathbuf().unwrap();  // TODO: fix unwrap
        let note_db_path_str = note_db_path.to_str().unwrap().to_string();  // TODO: fix unwrap

        // Spawn a new thread to monitor the filesystem.
        self.watcher_thread = Some(thread::spawn(move || {
            // Create a channel to receive the events.
            let (tx, rx) = mpsc::channel();

            let mut watcher = recommended_watcher(tx).unwrap();

            // Add a path to be watched. All files and directories at that path and
            // below will be monitored for changes.
            if let Err(e) = watcher.watch(&path, RecursiveMode::Recursive) {
                on_error(format!("Error watching path: {}", e));
                return;
            }

            loop {
                match rx.recv() {
                    Ok(event) => {
                        match event {
                            Ok(event) => {
                                for path in event.paths {
                                    if !path.is_file() {
                                        // not a file, loop to next iteration
                                        continue;
                                    }
                                    let filename_string = path.file_name().unwrap().to_str().unwrap();
                                    let pathname_string = path.parent().unwrap().to_str().unwrap();
                                    // only act if the change is for a yaml file
                                    if re.is_match(filename_string) {
                                        // then try to match the path of the db file to subpath to determine the type
                                        if pathname_string == task_db_path_str {
                                            on_change(DatabaseFileType::Task(filename_string.to_string()));
                                        } else if pathname_string == note_db_path_str {
                                            on_change(DatabaseFileType::Note(filename_string.to_string()));
                                        } else {
                                            on_error(format!("file changed in flatfile database, but its neither a Task or a Note: {filename_string}"));
                                        }
                                    }
                                }
                            },
                            Err(e) => on_error(format!("Event error: {:?}", e))
                        };
                    }
                    Err(e) => {
                        // Handle the error.
                        on_error(format!("Watch error: {:?}", e));
                    }
                }
            }
        }));
    }
}

impl Drop for FilesystemMonitor {
    fn drop(&mut self) {
        if let Some(handle) = self.watcher_thread.take() {
            handle.join().unwrap_or(());
        }
    }
}

//  eof
