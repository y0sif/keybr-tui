//! How keybr-tui writes and reads its user files (the stats files and
//! config.toml). Every write goes through [`write_atomic`], so a crash, a
//! closed terminal or a power cut mid-save leaves the old file or the new
//! one, never an empty one. Every read goes through [`read_user_file`], so
//! a file that is there but broken is moved aside instead of being
//! overwritten by the next save.

use std::ffi::OsString;
use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::localtime::unix_now_secs;

/// Replace `path` with `bytes` atomically: whenever the process dies,
/// `path` holds either its old contents or `bytes`, never a mix and never
/// nothing.
///
/// The bytes go to a new temporary file in the same directory (so the same
/// filesystem), which is synced to disk and then renamed over `path`. A
/// rename within one filesystem replaces the target in one step, on Unix
/// and on Windows alike. Missing parent directories are created. On any
/// failure the temporary file is removed and `path` is left as it was.
///
/// A symlink at `path` is followed, so a file a dotfile manager links into
/// place stays a link and the file it points at gets the new contents,
/// as it did when saves wrote through the link.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = follow_symlink(path);
    let dir = parent_dir(&target);
    fs::create_dir_all(dir)?;
    let (temp, file) = create_temp(dir, &target)?;
    if let Err(e) = fill_and_rename(file, bytes, &temp, &target) {
        // Best effort: the error worth reporting is the one above.
        let _ = fs::remove_file(&temp);
        return Err(e);
    }
    #[cfg(unix)]
    sync_dir(dir);
    Ok(())
}

/// Write and sync the temporary file, close it, and rename it over
/// `target`. Closing first matters on Windows, where an open file cannot
/// always be renamed.
fn fill_and_rename(mut file: File, bytes: &[u8], temp: &Path, target: &Path) -> io::Result<()> {
    // Keep the mode a user gave the old file (a private 0600, say). Set
    // before the contents go in, so they are never readable under the
    // looser mode a new file gets.
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(target) {
        file.set_permissions(meta.permissions())?;
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    rename_over(temp, target)
}

/// `fs::rename`, retried for a moment on Windows: a virus scanner, an
/// indexer or a sync client holding `to` open makes the rename fail
/// (access denied, or a sharing violation) until it lets go, usually
/// within milliseconds.
#[cfg(windows)]
fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    let mut delay_ms = 10;
    for _ in 0..5 {
        match fs::rename(from, to) {
            Err(e)
                if e.kind() == io::ErrorKind::PermissionDenied
                    || e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                delay_ms = (delay_ms * 2).min(100);
            }
            result => return result,
        }
    }
    fs::rename(from, to)
}

#[cfg(not(windows))]
fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

/// The file a symlink at `path` points at, or `path` itself. A dangling
/// link is replaced like a missing file.
fn follow_symlink(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        _ => path.to_path_buf(),
    }
}

/// The directory `path` lives in, `.` for a bare file name.
fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

/// Create `.<name>.tmp-<pid>-<n>` next to `target`, with the first `n`
/// no other file has. `create_new` is what makes the name unique: two
/// writers can never share a temporary file, and a stale one a killed
/// process left behind is skipped, never reused.
fn create_temp(dir: &Path, target: &Path) -> io::Result<(PathBuf, File)> {
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name to write"))?;
    let pid = std::process::id();
    let mut n: u32 = 0;
    loop {
        let mut temp_name = OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".tmp-{pid}-{n}"));
        let temp = dir.join(temp_name);
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && n < 1000 => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// Sync the directory, so the rename itself survives a power cut.
///
/// Best effort: the rename has already happened, so the file holds the new
/// contents either way, and some filesystems refuse to sync a directory.
/// Failing the save here would report a save that did reach the file.
/// Windows has no equivalent and needs none (NTFS journals the rename).
#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Ok(dir) = File::open(dir) {
        let _ = dir.sync_all();
    }
}

/// What reading a user file found.
#[derive(Debug)]
pub enum Loaded<T> {
    /// No file there, as on a first run. Not worth a warning.
    Missing,
    /// Read and parsed.
    Found(T),
    /// The file is there but could not be read or parsed. It has been
    /// moved aside so no save overwrites it (or could not be); the warning
    /// says which, and where it went.
    Broken(String),
}

impl<T> Loaded<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Loaded<U> {
        match self {
            Loaded::Missing => Loaded::Missing,
            Loaded::Found(value) => Loaded::Found(f(value)),
            Loaded::Broken(warning) => Loaded::Broken(warning),
        }
    }

    /// The value, if one was found, handing a broken file's warning to
    /// `warn`.
    pub fn found_or(self, warn: impl FnOnce(String)) -> Option<T> {
        match self {
            Loaded::Missing => None,
            Loaded::Found(value) => Some(value),
            Loaded::Broken(warning) => {
                warn(warning);
                None
            }
        }
    }

    /// The value, if one was found, printing a broken file's warning to
    /// stderr. Only for loads before the TUI starts: under the alternate
    /// screen nobody would see it.
    pub fn found_or_print(self) -> Option<T> {
        self.found_or(|warning| eprintln!("{warning}"))
    }
}

/// Read the user file at `path` and parse it with `parse`.
///
/// A file that is missing is [`Loaded::Missing`]. A file that is there but
/// cannot be read or parsed is moved aside with [`move_aside`] before
/// [`Loaded::Broken`] is returned, so the save that follows writes a new
/// file instead of destroying what might still be recovered from the old
/// one. `what` names the file in the warning ("stats") and `fallback` says
/// what the app does instead ("Starting fresh.").
pub fn read_user_file<T, E: Display>(
    path: &Path,
    what: &str,
    fallback: &str,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Loaded<T> {
    let problem = match fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => format!("could not read {what} at {}: {e}", path.display()),
        Ok(contents) => match parse(&contents) {
            Ok(value) => return Loaded::Found(value),
            // trim_end: a TOML error ends in a newline after its snippet.
            Err(e) => format!(
                "malformed {what} at {}: {}",
                path.display(),
                e.to_string().trim_end()
            ),
        },
    };
    let warning = match move_aside(path, unix_now_secs().unwrap_or(0)) {
        Ok(aside) => format!(
            "Warning: {problem}. Moved it to {}. {fallback}",
            aside.display()
        ),
        Err(e) => format!(
            "Warning: {problem}. Could not move it aside ({e}), so a later save may \
             overwrite it: copy it somewhere safe. {fallback}"
        ),
    };
    Loaded::Broken(warning)
}

/// Rename the file at `path` to `<name>.corrupt-<now_secs>` in the same
/// directory (`-1`, `-2` and so on after it if that name is taken), and
/// return the new path. A rename keeps the bytes exactly as they were.
pub fn move_aside(path: &Path, now_secs: i64) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name to move"))?;
    for n in 0..1000 {
        let mut aside_name = name.to_os_string();
        aside_name.push(format!(".corrupt-{now_secs}"));
        if n > 0 {
            aside_name.push(format!("-{n}"));
        }
        let aside = path.with_file_name(aside_name);
        // `symlink_metadata` so that even a dangling link counts as taken.
        if fs::symlink_metadata(&aside).is_err() {
            fs::rename(path, &aside)?;
            return Ok(aside);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "too many earlier .corrupt copies",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::test_support::TestDir;

    #[test]
    fn write_atomic_creates_then_replaces_and_leaves_no_temp() {
        let dir = TestDir::new("atomic-round-trip");
        let path = dir.path().join("nested").join("stats.json");
        write_atomic(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        write_atomic(&path, b"second, longer than the first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second, longer than the first");
        write_atomic(&path, b"").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"");
        assert_eq!(dir.entries_in("nested"), ["stats.json"]);
    }

    #[test]
    fn write_over_a_directory_fails_and_leaves_it_and_no_temp() {
        // A target that cannot be replaced: the rename fails, so this
        // covers cleanup after the temporary file was already written.
        let dir = TestDir::new("atomic-over-dir");
        let path = dir.path().join("stats.json");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("inside"), b"keep me").unwrap();
        assert!(write_atomic(&path, b"new").is_err());
        assert_eq!(fs::read(path.join("inside")).unwrap(), b"keep me");
        assert_eq!(dir.entries(), ["stats.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn failed_write_in_a_read_only_dir_leaves_the_old_file_byte_for_byte() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no preconditions and cannot fail.
        if unsafe { libc::geteuid() } == 0 {
            // Root writes through directory permissions.
            return;
        }
        let dir = TestDir::new("atomic-read-only");
        let path = dir.path().join("stats.json");
        let old = br#"{"total_lessons": 41}"#;
        fs::write(&path, old).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let result = write_atomic(&path, b"new");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), old);
        assert_eq!(dir.entries(), ["stats.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_keeps_the_mode_and_follows_a_symlink() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("atomic-symlink");
        let real = dir.path().join("dotfiles").join("config.toml");
        fs::create_dir(real.parent().unwrap()).unwrap();
        fs::write(&real, b"old").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join("config.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write_atomic(&link, b"new").unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&real).unwrap(), b"new");
        let mode = fs::metadata(&real).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(dir.entries_in("dotfiles"), ["config.toml"]);
    }

    #[test]
    fn move_aside_keeps_the_bytes_and_never_overwrites_an_earlier_copy() {
        let dir = TestDir::new("move-aside");
        let path = dir.path().join("stats.json");
        fs::write(&path, b"broken once").unwrap();
        let first = move_aside(&path, 1_700_000_000).unwrap();
        assert_eq!(first.file_name().unwrap(), "stats.json.corrupt-1700000000");
        fs::write(&path, b"broken twice").unwrap();
        let second = move_aside(&path, 1_700_000_000).unwrap();
        assert_eq!(
            second.file_name().unwrap(),
            "stats.json.corrupt-1700000000-1"
        );
        assert_eq!(fs::read(&first).unwrap(), b"broken once");
        assert_eq!(fs::read(&second).unwrap(), b"broken twice");
        assert!(!path.exists());
    }

    #[test]
    fn a_missing_file_is_silent_and_touches_nothing() {
        let dir = TestDir::new("read-missing");
        let path = dir.path().join("stats.json");
        let loaded = read_user_file(&path, "stats", "Starting fresh.", |s| s.parse::<u32>());
        assert!(matches!(loaded, Loaded::Missing));
        assert!(dir.entries().is_empty());
    }

    #[test]
    fn an_unreadable_file_is_moved_aside_too() {
        // A directory where the file should be cannot be read as one.
        let dir = TestDir::new("read-unreadable");
        let path = dir.path().join("stats.json");
        fs::create_dir(&path).unwrap();
        let loaded = read_user_file(&path, "stats", "Starting fresh.", |s| s.parse::<u32>());
        let Loaded::Broken(warning) = loaded else {
            panic!("expected Broken, got {loaded:?}");
        };
        assert!(warning.contains("could not read stats"), "{warning}");
        let entries = dir.entries();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert!(entries[0].starts_with("stats.json.corrupt-"), "{entries:?}");
    }

    /// Set in the child process `killed_mid_save_leaves_the_old_or_the_new_file`
    /// spawns, to the file the child should keep saving.
    #[cfg(unix)]
    const KILL_TEST_TARGET: &str = "KEYBR_TUI_KILL_TEST_TARGET";

    /// The generation number a kill-test file holds. Panics if the file
    /// does not parse, which is the failure the test is looking for.
    #[cfg(unix)]
    fn generation(path: &Path) -> u64 {
        let bytes = fs::read(path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "{} is not whole JSON ({e}): {} bytes",
                path.display(),
                bytes.len()
            )
        });
        value["generation"].as_u64().unwrap()
    }

    #[cfg(unix)]
    fn kill_test_contents(generation: u64) -> Vec<u8> {
        // Big enough that a write takes a while to land mid-way through.
        let pad = "x".repeat(32 * 1024);
        format!("{{\"generation\": {generation}, \"pad\": \"{pad}\"}}").into_bytes()
    }

    /// Kills the child if the test fails before it does.
    #[cfg(unix)]
    struct KillOnDrop(std::process::Child);

    #[cfg(unix)]
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[cfg(unix)]
    #[test]
    fn killed_mid_save_leaves_the_old_or_the_new_file() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        if let Some(target) = std::env::var_os(KILL_TEST_TARGET) {
            // The child: save newer and newer generations until killed
            // (or for 10 s, so a lost parent does not leave it spinning).
            let target = PathBuf::from(target);
            let start = Instant::now();
            let mut generation = generation(&target);
            while start.elapsed() < Duration::from_secs(10) {
                generation += 1;
                write_atomic(&target, &kill_test_contents(generation)).unwrap();
            }
            return;
        }

        let dir = TestDir::new("atomic-kill");
        let target = dir.path().join("stats.json");
        write_atomic(&target, &kill_test_contents(0)).unwrap();
        let module = module_path!().split_once("::").unwrap().1;
        let test_name = format!("{module}::killed_mid_save_leaves_the_old_or_the_new_file");

        let mut last = 0;
        let mut rng: u32 = 0x9e37_79b9;
        for round in 0..20 {
            let child = Command::new(std::env::current_exe().unwrap())
                .args([test_name.as_str(), "--exact", "--test-threads=1"])
                .env(KILL_TEST_TARGET, &target)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut child = KillOnDrop(child);

            // Wait until the child is saving, reading the file all the
            // while: a reader must never see a partial file either.
            let deadline = Instant::now() + Duration::from_secs(5);
            while generation(&target) == last {
                assert!(
                    Instant::now() < deadline,
                    "round {round}: the child never saved"
                );
                std::thread::sleep(Duration::from_micros(200));
            }
            // Then kill it at a varying point within the next 2 ms.
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            std::thread::sleep(Duration::from_micros(u64::from(rng % 2000)));
            child.0.kill().unwrap();
            child.0.wait().unwrap();

            let now = generation(&target);
            assert!(
                now > last,
                "round {round}: generation went from {last} to {now}"
            );
            last = now;
        }
    }
}
