//! Server diagnostics are best effort when stderr stops accepting bytes.
//!
//! A PHP request can call `error_log('message')` even when the process
//! reading the server log has paused. The request must not wait for that
//! reader, so records go to one writer thread through a queue that holds at
//! most [`MAX_QUEUED_BYTES`]. A record that does not fit is dropped, and the
//! writer says how many were lost as soon as it can write again:
//!
//! ```text
//! phera-server: 12 log records were dropped because the log was not being read
//! ```

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{
    OnceLock,
    mpsc::{SyncSender, sync_channel},
};
use std::time::Duration;

/// How much log text may wait for the writer: a burst of a few thousand
/// ordinary lines, without letting a stalled reader grow the process.
const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
/// More records than this never wait at once: even empty lines are counted.
const MAX_QUEUED_RECORDS: usize = 16 * 1024;

enum Record {
    Line(Vec<u8>),
    Flush(SyncSender<()>),
}

static SENDER: OnceLock<Option<SyncSender<Record>>> = OnceLock::new();
static QUEUED_BYTES: AtomicUsize = AtomicUsize::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Enqueue a complete web SAPI log record, dropping it when the queue is full.
pub fn write(line: Vec<u8>) {
    let size = line.len();
    let Some(sender) = SENDER.get_or_init(start) else {
        return;
    };
    // Reserve the bytes first; several requests log at the same time.
    let reserved = QUEUED_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    if reserved > MAX_QUEUED_BYTES || sender.try_send(Record::Line(line)).is_err() {
        QUEUED_BYTES.fetch_sub(size, Ordering::Relaxed);
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Give a readable sink a short opportunity to finish on normal server exit.
pub fn flush() {
    let Some(Some(sender)) = SENDER.get() else {
        return;
    };
    let (ready, finished) = sync_channel(1);
    if sender.try_send(Record::Flush(ready)).is_ok() {
        let _ = finished.recv_timeout(Duration::from_millis(50));
    }
}

fn start() -> Option<SyncSender<Record>> {
    let (sender, receiver) = sync_channel(MAX_QUEUED_RECORDS);
    // A blocked write stays on this detached thread. Tokio waits for its
    // blocking pool at shutdown, so using that pool could also stall exit.
    std::thread::Builder::new()
        .name("phera-web-log".into())
        .spawn(move || {
            for record in receiver {
                match record {
                    Record::Line(bytes) => {
                        let mut stderr = io::stderr().lock();
                        let _ = stderr.write_all(&bytes);
                        QUEUED_BYTES.fetch_sub(bytes.len(), Ordering::Relaxed);
                        report_dropped(&mut stderr);
                    }
                    Record::Flush(ready) => {
                        report_dropped(&mut io::stderr().lock());
                        let _ = ready.try_send(());
                    }
                }
            }
        })
        .ok()?;
    Some(sender)
}

/// Says once how many records were lost since the writer last got through.
fn report_dropped(stderr: &mut impl Write) {
    let dropped = DROPPED.swap(0, Ordering::Relaxed);
    if dropped > 0 {
        let _ = writeln!(
            stderr,
            "phera-server: {dropped} log records were dropped because the log was not being read"
        );
    }
}
