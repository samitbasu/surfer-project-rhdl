use camino::Utf8Path;
#[cfg(not(windows))]
use log::{error, info};
use notify::Error;
#[cfg(not(windows))]
use notify::{event::ModifyKind, Config, Event, EventKind, RecursiveMode, Watcher};
#[cfg(not(windows))]
use std::time::Duration;

/// Watches a provided file for changes.
/// Currently, this only works for Unix-like systems (tested on linux and macOS).
pub struct FileWatcher {
    #[cfg(not(windows))]
    _inner: notify::RecommendedWatcher,
}

/// Checks whether two paths, pointing at a file, refer to the same file.
/// This might be slower than some platform-dependent alternatives,
/// but should be guaranteed to work on all platforms
#[allow(dead_code)] // Only used in tests on Windows
fn is_same_file(p1: impl AsRef<std::path::Path>, p2: impl AsRef<std::path::Path>) -> bool {
    match (
        std::fs::canonicalize(p1.as_ref()),
        std::fs::canonicalize(p2.as_ref()),
    ) {
        (Ok(p1_canon), Ok(p2_canon)) => p1_canon == p2_canon,
        _ => false,
    }
}

#[cfg(not(windows))]
impl FileWatcher {
    /// Create a watcher for a path pointing to some file.
    /// Whenever that file changes, the provided `on_change` will be called.
    /// The returned `FileWatcher` will stop watching files when dropped.
    pub fn new<F>(path: &Utf8Path, on_change: F) -> Result<FileWatcher, Error>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let std_path = path.as_std_path().to_owned();
        let binding = std_path.clone();
        let Some(parent) = binding.parent() else {
            return Err(Error::new(notify::ErrorKind::PathNotFound).add_path(std_path));
        };
        let mut watcher = notify::RecommendedWatcher::new(
            move |res| match res {
                Ok(Event {
                    kind: EventKind::Modify(ModifyKind::Data(_)),
                    paths,
                    ..
                }) => {
                    if paths.iter().any(|path| is_same_file(path, &std_path)) {
                        info!("Observed file {} was changed on disk", std_path.display());
                        on_change();
                    }
                }
                Ok(_) => {}
                Err(e) => error!("Error while watching fil\n{}", e),
            },
            Config::default().with_poll_interval(Duration::from_secs(1)),
        )?;

        watcher.watch(parent, RecursiveMode::NonRecursive)?;
        info!("Watching file {} for changes", binding.display());

        Ok(FileWatcher { _inner: watcher })
    }
}

// Currently, the windows tests fail with `exit code: 0xc000001d, STATUS_ILLEGAL_INSTRUCTION`.
// It is not quite clear whether this issue originates with the `tempfile` crate or `notify`
// (see https://github.com/notify-rs/notify/issues/624). Therefore, the FileWatcher is a noop
// implementation for windows. Since, at the time of initial implementation,
// this issue couldn't be resolved, the file watcher only has partial support for unix-like
// systems
#[cfg(windows)]
impl FileWatcher {
    pub fn new<F>(_path: &Utf8Path, _on_change: F) -> Result<FileWatcher, Error>
    where
        F: Fn() + Send + Sync + 'static,
    {
        // blank implementation
        Ok(FileWatcher {})
    }
}

#[cfg(test)]
mod tests {
    use crate::file_watcher::{is_same_file, FileWatcher};
    use camino::Utf8Path;
    use std::fs;
    use std::fs::File;
    #[cfg(not(windows))]
    use std::fs::OpenOptions;
    #[cfg(not(windows))]
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    struct TempDir {
        inner: tempfile::TempDir,
    }

    impl TempDir {
        pub fn new() -> TempDir {
            TempDir {
                inner: tempfile::TempDir::new().unwrap(),
            }
        }

        pub fn create(&self, file: &str) -> PathBuf {
            let file_path = self.path().join(file);
            File::create(&file_path).unwrap();
            file_path
        }

        pub fn mkdir(&self, name: &str) -> PathBuf {
            let file_path = self.path().join(name);
            fs::create_dir(&file_path).unwrap();
            file_path
        }

        pub fn path(&self) -> &Path {
            self.inner.path()
        }
    }

    /// Guard ensuring that a callback is executed.
    pub struct CallbackGuard {
        called: Mutex<bool>,
        lock: Condvar,
    }

    impl CallbackGuard {
        pub fn new() -> Arc<Self> {
            Arc::new(CallbackGuard {
                called: Mutex::new(false),
                lock: Condvar::new(),
            })
        }

        pub fn signal(&self) {
            let mut guard = self.called.lock().unwrap();
            *guard = true;
            self.lock.notify_all();
        }

        /// Block until signal has been called or a timeout occurred.
        /// Panics on the timeout.
        #[cfg(not(windows))]
        pub fn assert_called(&self) {
            let mut started = self.called.lock().unwrap();
            let result = self
                .lock
                .wait_timeout(started, Duration::from_secs(10))
                .unwrap();
            started = result.0;
            if *started == true {
                // We received the notification and the value has been updated, we can leave.
                return;
            }
            panic!("Timeout while waiting for callback")
        }

        /// Block until signal has been called or a timeout occurred.
        /// Panics when the signal has been called.
        pub fn assert_not_called(&self) {
            let mut started = self.called.lock().unwrap();
            let result = self
                .lock
                .wait_timeout(started, Duration::from_secs(10))
                .unwrap();
            started = result.0;
            if *started == true {
                panic!("Callback was called");
            }
        }
    }

    #[test]
    #[cfg(not(windows))]
    pub fn notifies_on_change() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new();
        let path = tmp_dir.create("test");

        let barrier = CallbackGuard::new();
        let barrier_clone = barrier.clone();
        let _watcher = FileWatcher::new(Utf8Path::from_path(path.as_ref()).unwrap(), move || {
            barrier_clone.signal();
        });
        {
            // We open, write and close a file. The observer should have been called.
            let mut file = OpenOptions::new().write(true).open(&path)?;
            writeln!(file, "Changes")?;
        }
        barrier.assert_called();
        Ok(())
    }

    #[test]
    pub fn does_not_notify_on_create_and_delete() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new();
        let path = tmp_dir.path().join("test");

        let barrier = CallbackGuard::new();
        let barrier_clone = barrier.clone();

        let _watcher = FileWatcher::new(Utf8Path::from_path(path.as_ref()).unwrap(), move || {
            barrier_clone.signal();
        });
        {
            // open a file
            File::create(&path)?;
        }
        {
            // delete the file
            fs::remove_file(path)?;
        }
        barrier.assert_not_called();
        Ok(())
    }

    #[test]
    #[cfg(not(windows))]
    pub fn resolves_files_that_are_named_differently() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new();
        let mut path = tmp_dir.mkdir("test");
        path.push("test_file");
        File::create(&path).unwrap();

        let barrier = CallbackGuard::new();
        let barrier_clone = barrier.clone();

        let _watcher = FileWatcher::new(
            &Utf8Path::from_path(tmp_dir.path())
                .unwrap()
                .join("test/test_file"),
            move || {
                barrier_clone.signal();
            },
        );
        {
            // We open, write and close a file. The observer should have been called.
            let mut file = OpenOptions::new().write(true).open(&path)?;
            writeln!(file, "Changes")?;
        }
        barrier.assert_called();
        Ok(())
    }

    #[test]
    pub fn check_file_for_difference() {
        let tmp_dir = TempDir::new();
        let file1 = tmp_dir.create("file1");
        let dir = tmp_dir.mkdir("dir");
        let mut file2 = dir.clone();
        file2.push("file2");
        File::create(&file2).unwrap();
        assert!(is_same_file(&file1, &file1));
        assert!(!is_same_file(&file1, &file2));
        let mut complicated_file2 = dir.clone();
        complicated_file2.push("../dir/file2");
        assert!(is_same_file(&complicated_file2, &file2))
    }
}
