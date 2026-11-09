//! The background writer: in-app saves reach the disk on their own thread,
//! so a slow write (an fsync can take tens of milliseconds on a busy
//! filesystem) does not stall a keystroke or a frame. Only a language switch
//! waits for it (see below).
//!
//! The UI thread serializes (cheap) and queues `(path, bytes)`; the writer
//! thread writes each through [`write_atomic`]. Every save is written there.
//! The one other change to a user file while the TUI runs, a language switch
//! moving a broken stats file aside, happens on the UI thread after
//! [`Saver::flush`], while the writer is idle.

use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use super::files::write_atomic;

/// Owns the writer thread. Created by `main` before the TUI starts and
/// finished after it ends; the App only borrows it to queue saves.
///
/// Ordering: one thread works through one queue, so writes happen in the
/// order they were queued. When several writes of one file are waiting,
/// only the latest is written, since each is the whole file and the
/// latest supersedes the rest. An older write can never land over a newer
/// one.
pub struct Saver {
    jobs: Option<Sender<Job>>,
    /// The queue for [`PanicFlush`]. Emptied when the Saver stops, so a
    /// panic hook that outlives it never keeps the writer waiting.
    panic_jobs: Arc<Mutex<Option<Sender<Job>>>>,
    writer: Option<JoinHandle<()>>,
    report: Arc<Mutex<Report>>,
}

/// The writer thread's name. A panic on it must not wait for itself (see
/// [`PanicFlush::wait`]).
const WRITER_THREAD: &str = "keybr-tui-saver";

enum Job {
    Save(Save),
    /// Answered once every save queued before it is done.
    Barrier(Sender<()>),
}

/// One save of one file: the bytes to replace it with, or, for contents
/// that would not serialize, why not. Either way it is that file's latest
/// save, so a failure goes through the queue too, in order with the rest.
struct Save {
    /// Names the file in a failure message ("stats", "config").
    what: &'static str,
    path: PathBuf,
    contents: Result<Vec<u8>, String>,
}

/// What went wrong, collected for after the TUI is gone.
#[derive(Default)]
struct Report {
    /// One per file whose latest save failed, with the message for it.
    failures: Vec<(PathBuf, String)>,
    /// Failures no later save can make up for, because the contents that
    /// failed to save were dropped (see [`Saver::abandon`]).
    lost: Vec<String>,
    warnings: Vec<String>,
}

impl Report {
    /// Record the outcome of saving `path`. A success clears an earlier
    /// failure of the same file: the save that failed held an older copy
    /// of the same whole file, which the one that succeeded replaced.
    fn record(&mut self, path: &Path, outcome: Result<(), String>) {
        self.failures.retain(|(failed, _)| failed != path);
        if let Err(message) = outcome {
            self.failures.push((path.to_path_buf(), message));
        }
    }

    /// Keep a failure of `path` for exit whatever later saves of it do.
    fn abandon(&mut self, path: &Path) {
        let (gone, kept) = std::mem::take(&mut self.failures)
            .into_iter()
            .partition(|(failed, _)| failed == path);
        self.failures = kept;
        self.lost
            .extend(gone.into_iter().map(|(_, message)| message));
    }

    /// Every failure to print, oldest first and each message once (a lost
    /// save and a later failed save of the same file read the same).
    fn take_failures(&mut self) -> Vec<String> {
        let failures = std::mem::take(&mut self.failures);
        let mut messages: Vec<String> = Vec::new();
        for message in std::mem::take(&mut self.lost)
            .into_iter()
            .chain(failures.into_iter().map(|(_, message)| message))
        {
            if !messages.contains(&message) {
                messages.push(message);
            }
        }
        messages
    }
}

/// What `main` prints once the terminal is restored.
#[derive(Debug, Default)]
pub struct SaveReport {
    /// Problems that lost nothing, such as a broken stats file found and
    /// moved aside on a language switch.
    pub warnings: Vec<String>,
    /// Saves that did not reach the disk. Any of these makes the process
    /// exit non-zero.
    pub failures: Vec<String>,
}

impl Saver {
    /// Start the writer thread.
    pub fn spawn() -> io::Result<Saver> {
        Saver::spawn_with(write_atomic)
    }

    /// Start a writer thread that writes with `write` (tests swap in one
    /// they can watch, slow down or make fail).
    pub(crate) fn spawn_with<W>(write: W) -> io::Result<Saver>
    where
        W: FnMut(&Path, &[u8]) -> io::Result<()> + Send + 'static,
    {
        let (jobs, queue) = mpsc::channel();
        let report = Arc::new(Mutex::new(Report::default()));
        let writer_report = Arc::clone(&report);
        let writer = std::thread::Builder::new()
            .name(WRITER_THREAD.to_string())
            .spawn(move || run_writer(queue, &writer_report, write))?;
        Ok(Saver {
            panic_jobs: Arc::new(Mutex::new(Some(jobs.clone()))),
            jobs: Some(jobs),
            writer: Some(writer),
            report,
        })
    }

    /// A handle a panic hook can use to wait for the saves queued so far.
    pub fn panic_flush(&self) -> PanicFlush {
        PanicFlush(Arc::clone(&self.panic_jobs))
    }

    /// Queue `bytes` to replace the file at `path`. `what` names it in a
    /// failure message ("stats", "config"). Returns at once.
    pub fn write(&self, what: &'static str, path: PathBuf, bytes: Vec<u8>) {
        self.queue(what, path, Ok(bytes));
    }

    /// Report a save of `path` that failed before it could be queued,
    /// because its contents would not serialize.
    pub fn fail(&self, what: &'static str, path: PathBuf, error: impl Display) {
        self.queue(what, path, Err(error.to_string()));
    }

    fn queue(&self, what: &'static str, path: PathBuf, contents: Result<Vec<u8>, String>) {
        let Some(jobs) = &self.jobs else { return };
        let save = Save {
            what,
            path,
            contents,
        };
        if let Err(mpsc::SendError(Job::Save(save))) = jobs.send(Job::Save(save)) {
            let message = save_failure(save.what, &save.path, "the save thread has stopped");
            self.lock().record(&save.path, Err(message));
        }
    }

    /// Keep a warning to print once the TUI is gone.
    pub fn warn(&self, warning: String) {
        self.lock().warnings.push(warning);
    }

    /// Block until every write queued so far is on disk (or has failed).
    /// Call it before reading a file a queued write might replace.
    pub fn flush(&self) {
        let Some(jobs) = &self.jobs else { return };
        let (done, wait) = mpsc::channel();
        if jobs.send(Job::Barrier(done)).is_ok() {
            // An error means the writer is gone, and nothing is pending.
            let _ = wait.recv();
        }
    }

    /// The App is about to drop its contents for `path` and read the file
    /// back from disk (a language switch does). Call it after [`flush`],
    /// so the latest save of `path` has run. If that save failed, what it
    /// held is lost: a later save of `path` writes what was read back, not
    /// what failed, so it must not clear the failure. It stays reported.
    ///
    /// [`flush`]: Saver::flush
    pub fn abandon(&self, path: &Path) {
        self.lock().abandon(path);
    }

    /// Wait for every queued write to reach the disk, stop the thread, and
    /// return what went wrong.
    pub fn finish(mut self) -> SaveReport {
        self.stop();
        let mut report = self.lock();
        SaveReport {
            warnings: std::mem::take(&mut report.warnings),
            failures: report.take_failures(),
        }
    }

    /// Close the queue and wait for the writer to drain it.
    fn stop(&mut self) {
        drop(self.jobs.take());
        drop(
            self.panic_jobs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        if let Some(writer) = self.writer.take() {
            if writer.join().is_err() {
                self.lock().failures.push((
                    PathBuf::new(),
                    "Error: the save thread crashed, so some saves may be lost".to_string(),
                ));
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Report> {
        lock(&self.report)
    }
}

/// `main` calls `finish`; this covers leaving early on an error. Queued
/// saves still reach the disk, and what went wrong is still printed.
impl Drop for Saver {
    fn drop(&mut self) {
        self.stop();
        let mut report = self.lock();
        for warning in std::mem::take(&mut report.warnings) {
            eprintln!("{warning}");
        }
        for failure in report.take_failures() {
            eprintln!("{failure}");
        }
    }
}

/// Waits for the saves queued before a panic. With `panic = "abort"` (the
/// release profile) the process ends when the panic hook returns, before
/// `main` can wait for the [`Saver`], so the save a quit or a lesson just
/// queued would go with it. `main`'s panic hook waits here instead.
#[derive(Clone)]
pub struct PanicFlush(Arc<Mutex<Option<Sender<Job>>>>);

impl PanicFlush {
    /// Block until every save queued so far is on disk (or has failed), or
    /// `timeout` passes. Returns at once on the writer thread itself, which
    /// cannot wait for itself, and once the Saver has stopped.
    pub fn wait(&self, timeout: Duration) {
        if std::thread::current().name() == Some(WRITER_THREAD) {
            return;
        }
        let jobs = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(jobs) = jobs else { return };
        let (done, wait) = mpsc::channel();
        if jobs.send(Job::Barrier(done)).is_ok() {
            drop(jobs);
            let _ = wait.recv_timeout(timeout);
        }
    }
}

/// The writer thread: take whatever is queued, write the latest bytes for
/// each file, answer the barriers, and wait for more. Ends when the queue
/// is closed and empty.
fn run_writer<W>(queue: Receiver<Job>, report: &Mutex<Report>, mut write: W)
where
    W: FnMut(&Path, &[u8]) -> io::Result<()>,
{
    while let Ok(first) = queue.recv() {
        // The latest save of each file, in the order the files first came.
        let mut saves: Vec<Save> = Vec::new();
        let mut barriers = Vec::new();
        for job in std::iter::once(first).chain(queue.try_iter()) {
            match job {
                Job::Save(save) => match saves.iter_mut().find(|q| q.path == save.path) {
                    Some(older) => *older = save,
                    None => saves.push(save),
                },
                Job::Barrier(done) => barriers.push(done),
            }
        }
        for Save {
            what,
            path,
            contents,
        } in saves
        {
            let outcome = match contents {
                Ok(bytes) => write(&path, &bytes).map_err(|e| save_failure(what, &path, e)),
                Err(error) => Err(save_failure(what, &path, error)),
            };
            lock(report).record(&path, outcome);
        }
        // Every write queued before these barriers was in this batch.
        for done in barriers {
            let _ = done.send(());
        }
    }
}

fn save_failure(what: &str, path: &Path, error: impl Display) -> String {
    format!(
        "Error: could not save {what} to {}: {error}",
        path.display()
    )
}

/// The report survives a panic elsewhere: it is a plain list, never left
/// half updated.
fn lock(report: &Mutex<Report>) -> MutexGuard<'_, Report> {
    report.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::test_support::TestDir;
    use std::fs;
    use std::time::Duration;

    type Log = Arc<Mutex<Vec<(PathBuf, Vec<u8>)>>>;

    /// A writer that only records what it was asked to write.
    fn recording() -> (Log, impl FnMut(&Path, &[u8]) -> io::Result<()> + Send) {
        let log: Log = Arc::default();
        let writer_log = Arc::clone(&log);
        let write = move |path: &Path, bytes: &[u8]| {
            writer_log
                .lock()
                .unwrap()
                .push((path.to_path_buf(), bytes.to_vec()));
            Ok(())
        };
        (log, write)
    }

    fn written(log: &Log) -> Vec<(String, String)> {
        log.lock()
            .unwrap()
            .iter()
            .map(|(p, b)| {
                (
                    p.display().to_string(),
                    String::from_utf8(b.clone()).unwrap(),
                )
            })
            .collect()
    }

    fn pair(path: &str, bytes: &str) -> (String, String) {
        (path.to_string(), bytes.to_string())
    }

    #[test]
    fn writes_land_in_order_and_queued_writes_of_a_file_collapse_to_the_latest() {
        let (log, mut record) = recording();
        let (started, wait_started) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        let mut first = true;
        // Holds the first write until the test has queued the rest, so
        // they are all waiting at once.
        let saver = Saver::spawn_with(move |path: &Path, bytes: &[u8]| {
            record(path, bytes)?;
            if std::mem::take(&mut first) {
                started.send(()).unwrap();
                gate.recv().unwrap();
            }
            Ok(())
        })
        .unwrap();

        saver.write("stats", "a".into(), b"a1".to_vec());
        wait_started.recv().unwrap();
        saver.write("stats", "a".into(), b"a2".to_vec());
        saver.write("config", "b".into(), b"b1".to_vec());
        saver.write("stats", "a".into(), b"a3".to_vec());
        saver.write("stats", "a".into(), b"a4".to_vec());
        release.send(()).unwrap();
        saver.flush();

        assert_eq!(
            written(&log),
            [pair("a", "a1"), pair("a", "a4"), pair("b", "b1")]
        );
        let report = saver.finish();
        assert!(report.failures.is_empty(), "{report:?}");
    }

    #[test]
    fn flush_waits_until_every_queued_write_is_on_disk() {
        let dir = TestDir::new("saver-flush");
        let saver = Saver::spawn_with(|path: &Path, bytes: &[u8]| {
            std::thread::sleep(Duration::from_millis(30));
            write_atomic(path, bytes)
        })
        .unwrap();
        let a = dir.path().join("stats.json");
        let b = dir.path().join("stats-de.json");
        saver.write("stats", a.clone(), b"old".to_vec());
        saver.write("stats", b.clone(), b"other".to_vec());
        saver.write("stats", a.clone(), b"new".to_vec());
        saver.flush();
        assert_eq!(fs::read(&a).unwrap(), b"new");
        assert_eq!(fs::read(&b).unwrap(), b"other");
        assert!(saver.finish().failures.is_empty());
    }

    #[test]
    fn finish_waits_for_queued_writes() {
        let dir = TestDir::new("saver-finish");
        let saver = Saver::spawn_with(|path: &Path, bytes: &[u8]| {
            std::thread::sleep(Duration::from_millis(30));
            write_atomic(path, bytes)
        })
        .unwrap();
        let path = dir.path().join("config.toml");
        saver.write("config", path.clone(), b"target_wpm = 40\n".to_vec());
        let report = saver.finish();
        assert!(report.failures.is_empty(), "{report:?}");
        assert_eq!(fs::read(&path).unwrap(), b"target_wpm = 40\n");
    }

    #[test]
    fn a_failed_write_is_reported_by_finish() {
        let dir = TestDir::new("saver-failure");
        // A directory where the file should be: the rename cannot replace it.
        let blocked = dir.path().join("stats.json");
        fs::create_dir(&blocked).unwrap();
        let fine = dir.path().join("config.toml");

        let saver = Saver::spawn().unwrap();
        saver.write("stats", blocked.clone(), b"{}".to_vec());
        saver.write("config", fine.clone(), b"".to_vec());
        saver.warn("Warning: something to tell".to_string());
        let report = saver.finish();

        assert_eq!(report.failures.len(), 1, "{report:?}");
        let failure = &report.failures[0];
        assert!(
            failure.starts_with("Error: could not save stats to "),
            "{failure}"
        );
        assert!(
            failure.contains(&blocked.display().to_string()),
            "{failure}"
        );
        assert_eq!(report.warnings, ["Warning: something to tell"]);
        assert!(fine.exists());
        assert!(blocked.is_dir());
    }

    #[test]
    fn a_later_successful_save_of_the_same_file_clears_its_failure() {
        let saver = Saver::spawn_with(|_: &Path, bytes: &[u8]| {
            if bytes == b"bad" {
                Err(io::Error::other("disk full"))
            } else {
                Ok(())
            }
        })
        .unwrap();
        saver.write("stats", "a".into(), b"bad".to_vec());
        saver.flush();
        saver.write("stats", "a".into(), b"good".to_vec());
        saver.write("config", "b".into(), b"bad".to_vec());
        saver.fail("config", "c".into(), "will not serialize");
        saver.write("config", "d".into(), b"fine".to_vec());
        saver.fail("config", "d".into(), "will not serialize");
        let report = saver.finish();
        assert_eq!(
            report.failures,
            [
                "Error: could not save config to b: disk full",
                "Error: could not save config to c: will not serialize",
                "Error: could not save config to d: will not serialize",
            ],
            "a failure queued after a write of the same file outranks it"
        );
    }

    /// Fails to write the bytes `bad`, and only those.
    fn fail_on_bad(_: &Path, bytes: &[u8]) -> io::Result<()> {
        if bytes == b"bad" {
            Err(io::Error::other("disk full"))
        } else {
            Ok(())
        }
    }

    #[test]
    fn an_abandoned_failure_stays_reported_after_a_later_save_succeeds() {
        let saver = Saver::spawn_with(fail_on_bad).unwrap();
        saver.write("stats", "a".into(), b"bad".to_vec());
        saver.write("stats", "b".into(), b"bad".to_vec());
        saver.write("stats", "c".into(), b"good".to_vec());
        saver.flush();
        // A language switch drops what "a" held and reads "a" back from
        // disk, so a later save of "a" holds something else.
        saver.abandon(Path::new("a"));
        // "c" saved fine: there is nothing of it to keep.
        saver.abandon(Path::new("c"));
        saver.write("stats", "a".into(), b"good".to_vec());
        // "b" was not read back: its later save holds the same contents,
        // so the passing problem is made up for.
        saver.write("stats", "b".into(), b"good".to_vec());
        saver.write("stats", "c".into(), b"good".to_vec());
        assert_eq!(
            saver.finish().failures,
            ["Error: could not save stats to a: disk full"]
        );
    }

    #[test]
    fn an_abandoned_file_that_fails_again_is_reported_once() {
        let saver = Saver::spawn_with(fail_on_bad).unwrap();
        saver.write("stats", "a".into(), b"bad".to_vec());
        saver.flush();
        saver.abandon(Path::new("a"));
        saver.write("stats", "a".into(), b"bad".to_vec());
        saver.write("config", "b".into(), b"bad".to_vec());
        assert_eq!(
            saver.finish().failures,
            [
                "Error: could not save stats to a: disk full",
                "Error: could not save config to b: disk full",
            ]
        );
    }

    #[test]
    fn panic_flush_waits_for_queued_saves_but_never_on_the_writer_or_after_finish() {
        let (log, mut record) = recording();
        let (release, gate) = mpsc::channel::<()>();
        // Holds every write until the test releases it.
        let saver = Saver::spawn_with(move |path: &Path, bytes: &[u8]| {
            gate.recv().unwrap();
            record(path, bytes)
        })
        .unwrap();
        let flush = saver.panic_flush();
        saver.write("stats", "a".into(), b"a1".to_vec());

        // On the writer thread itself, waiting would only run out the clock.
        let on_writer = flush.clone();
        let waited = std::thread::Builder::new()
            .name(WRITER_THREAD.to_string())
            .spawn(move || {
                let start = std::time::Instant::now();
                on_writer.wait(Duration::from_secs(30));
                start.elapsed()
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(waited < Duration::from_secs(5), "{waited:?}");

        release.send(()).unwrap();
        flush.wait(Duration::from_secs(30));
        assert_eq!(written(&log), [pair("a", "a1")]);

        // The handle holds no queue of its own, so finish does not wait on
        // it, and after finish it returns at once.
        assert!(saver.finish().failures.is_empty());
        let start = std::time::Instant::now();
        flush.wait(Duration::from_secs(30));
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
