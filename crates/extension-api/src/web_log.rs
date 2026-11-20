//! Server diagnostics are best effort when stderr stops accepting bytes.
//!
//! A PHP request can call `error_log('message')` even when the process
//! reading the server log has paused.

use std::io::{self, Write};
use std::sync::{
    OnceLock,
    mpsc::{SyncSender, sync_channel},
};
use std::time::Duration;

const QUEUE_CAPACITY: usize = 64;
const MAX_RECORD_BYTES: usize = 64 * 1024;

enum Record {
    Line(Vec<u8>),
    Flush(SyncSender<()>),
}

static SENDER: OnceLock<Option<SyncSender<Record>>> = OnceLock::new();

/// Enqueue a complete web SAPI log record, dropping it when the queue is full.
pub fn write(line: Vec<u8>) {
    if line.len() > MAX_RECORD_BYTES {
        return;
    }
    let sender = SENDER.get_or_init(start);
    if let Some(sender) = sender {
        let _ = sender.try_send(Record::Line(line));
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
    let (sender, receiver) = sync_channel(QUEUE_CAPACITY);
    // A blocked write stays on this detached thread. Tokio waits for its
    // blocking pool at shutdown, so using that pool could also stall exit.
    std::thread::Builder::new()
        .name("phera-web-log".into())
        .spawn(move || {
            for record in receiver {
                match record {
                    Record::Line(bytes) => {
                        let _ = io::stderr().write_all(&bytes);
                    }
                    Record::Flush(ready) => {
                        let _ = ready.try_send(());
                    }
                }
            }
        })
        .ok()?;
    Some(sender)
}
