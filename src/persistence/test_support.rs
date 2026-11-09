//! A throwaway directory for tests that touch the disk, so no test ever
//! reads or writes the user's real data or config directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A fresh directory under the system temp dir, removed on drop.
pub struct TestDir(PathBuf);

impl TestDir {
    /// `name` keeps tests apart within one run; the pid and the clock keep
    /// runs apart.
    pub fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "keybr-tui-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TestDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The names in the directory, sorted.
    pub fn entries(&self) -> Vec<String> {
        self.entries_in("")
    }

    /// The names in the subdirectory `sub`, sorted.
    pub fn entries_in(&self, sub: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.0.join(sub))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The one file in the directory whose name starts with `prefix`.
    pub fn only_file_starting_with(&self, prefix: &str) -> PathBuf {
        let matching: Vec<String> = self
            .entries()
            .into_iter()
            .filter(|name| name.starts_with(prefix))
            .collect();
        assert_eq!(matching.len(), 1, "{matching:?}");
        self.0.join(&matching[0])
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
