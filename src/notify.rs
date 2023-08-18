use notify::{RecursiveMode, recommended_watcher, Watcher, Event};
use std::sync::mpsc;
use std::thread;
use crate::settings::Settings;

/// Function type for change callback.
pub type ChangeCallback = fn(Event);

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
        let path = settings.as_ref().db_pathbuf().unwrap();

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
                            Ok(event) => on_change(event),
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
