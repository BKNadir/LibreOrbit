//! Log files: one folder per run, `<name>` files of `HH:MM:SS text` lines (like OpenOrbit's servers).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug)]
pub struct LogDir {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl LogDir {
    /// Creates the folder if needed.
    pub fn new(dir: impl Into<PathBuf>) -> std::io::Result<LogDir> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(LogDir {
            dir,
            lock: Mutex::new(()),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Appends `HH:MM:SS text` (local time) to `<dir>/<name>`. Errors are ignored: logging never stops a server.
    pub fn log(&self, name: &str, text: &str) {
        let line = format!("{} {text}\n", chrono::Local::now().format("%H:%M:%S"));
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(name))
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}
