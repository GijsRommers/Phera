//! Where a logger's lines go.
//!
//! - `stderr` and `stdout`: one `write` per record, straight to the
//!   process's stream, as `php://stderr` does. A line is never split.
//! - a file: lines go to a writer thread of that file, which appends them
//!   in batches and flushes as soon as it has no more to write. Logging
//!   costs the request a channel send, not a disk write. Every logger of
//!   one file (in any worker) shares that one thread, so lines never mix.
//! - `memory`: records are kept for `$logger->records()`, for tests.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use phera_extension_api::Value;

use super::record::Level;

pub(super) enum Sink {
    Stderr,
    Stdout,
    File(FileSink),
    Memory(RefCell<Vec<Kept>>),
}

/// A record `memory` keeps: plain data, the line it would have written.
pub(super) struct Kept {
    pub(super) time: String,
    pub(super) level: Level,
    pub(super) message: Value,
    pub(super) context: Value,
    pub(super) line: Value,
}

impl Sink {
    pub(super) fn write(&self, line: Vec<u8>) {
        match self {
            // A log line that cannot be written has nowhere to be reported.
            Sink::Stderr => {
                let _ = std::io::stderr().lock().write_all(&line);
            }
            Sink::Stdout => {
                let mut stdout = std::io::stdout().lock();
                let _ = stdout.write_all(&line);
                let _ = stdout.flush();
            }
            Sink::File(file) => file.write(line),
            Sink::Memory(_) => {}
        }
    }

    /// Waits until every line written so far is in the file.
    pub(super) fn flush(&self) {
        match self {
            Sink::Stderr | Sink::Memory(_) => {}
            Sink::Stdout => {
                let _ = std::io::stdout().lock().flush();
            }
            Sink::File(file) => file.flush(),
        }
    }
}

enum Message {
    Line(Vec<u8>),
    /// Answered once every line sent before it is written and flushed.
    Flush(mpsc::SyncSender<()>),
}

/// One logger's handle on the writer thread of its file.
pub(super) struct FileSink {
    path: PathBuf,
    sender: Sender<Message>,
    /// Lines were sent since the last flush.
    unflushed: Cell<bool>,
}

impl FileSink {
    /// Opens `path` for appending (creating it), or joins the writer that
    /// already has it open.
    pub(super) fn open(path: &Path) -> std::io::Result<FileSink> {
        // `a/./b.log` and `a/b.log` share one writer.
        let path = std::path::absolute(path)?;
        let mut writers = writers().lock().unwrap_or_else(|error| error.into_inner());
        if let Some(sender) = writers.get(&path) {
            return Ok(FileSink::with_sender(path, sender.clone()));
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("phera-log-writer".into())
            .spawn(move || write_lines(file, receiver))?;
        writers.insert(path.clone(), sender.clone());
        Ok(FileSink::with_sender(path, sender))
    }

    fn with_sender(path: PathBuf, sender: Sender<Message>) -> FileSink {
        FileSink {
            path,
            sender,
            unflushed: Cell::new(false),
        }
    }

    /// The same file for a fork of a snapshot.
    pub(super) fn reopen(&self) -> FileSink {
        FileSink::with_sender(self.path.clone(), self.sender.clone())
    }

    fn write(&self, line: Vec<u8>) {
        // The writer only stops when the process does.
        let _ = self.sender.send(Message::Line(line));
        self.unflushed.set(true);
    }

    fn flush(&self) {
        if !self.unflushed.replace(false) {
            return;
        }
        let (done, wait) = mpsc::sync_channel(1);
        if self.sender.send(Message::Flush(done)).is_ok() {
            let _ = wait.recv();
        }
    }
}

/// A script that ends right after logging still has its lines in the file.
impl Drop for FileSink {
    fn drop(&mut self) {
        self.flush();
    }
}

/// The writer of each open log file, by absolute path.
fn writers() -> &'static Mutex<HashMap<PathBuf, Sender<Message>>> {
    static WRITERS: OnceLock<Mutex<HashMap<PathBuf, Sender<Message>>>> = OnceLock::new();
    WRITERS.get_or_init(Mutex::default)
}

/// The writer thread: takes every line waiting, writes them in one go, and
/// flushes before it waits again, so a quiet logger's lines are on disk at
/// once and a busy one's go out in large writes.
fn write_lines(file: File, receiver: Receiver<Message>) {
    let mut out = BufWriter::with_capacity(64 * 1024, file);
    let mut waiting = Vec::new();
    while let Ok(first) = receiver.recv() {
        for message in std::iter::once(first).chain(receiver.try_iter()) {
            match message {
                Message::Line(line) => {
                    let _ = out.write_all(&line);
                }
                Message::Flush(done) => waiting.push(done),
            }
        }
        let _ = out.flush();
        for done in waiting.drain(..) {
            let _ = done.send(());
        }
    }
}
