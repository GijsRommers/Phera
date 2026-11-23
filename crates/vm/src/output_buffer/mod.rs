//! `ob_start(); echo "template"; $html = ob_get_clean();` captures output.
//!
//! PHP runs shutdown functions, then request-end destructors, then finalizes
//! buffers from the innermost out. The last buffer commits the response head
//! when it delivers bytes. Phera runs shutdown functions immediately before
//! finalizing the buffers, including live objects created during timeout
//! shutdown. Ordinary object destructor dispatch is pending.
//!
//! A callback can call back into the VM, including reading the active buffer.
//! The stack owns all buffer data and no reference into it crosses that call.

mod deprecation;
mod functions;
mod operations;

use phera_value::{ArrayKey, PhpArray, Value};

use crate::calls::call_state::Callee;

pub(crate) use functions::OutputFunction;
pub(super) use operations::Operation;

const CLEANABLE: i64 = 0x10;
const FLUSHABLE: i64 = 0x20;
const REMOVABLE: i64 = 0x40;
pub(crate) const STDFLAGS: i64 = CLEANABLE | FLUSHABLE | REMOVABLE;
const STARTED: i64 = 0x1000;
const DISABLED: i64 = 0x2000;
const PROCESSED: i64 = 0x4000;
const PRODUCED_OUTPUT: i64 = 0x8000;
const CHUNK_ALIGNMENT: usize = 4096;
const DEFAULT_SIZE: usize = 16384;
const DEFAULT_NAME: &str = "default output handler";

pub(crate) const CONSTANTS: &[(&str, i64)] = &[
    ("PHP_OUTPUT_HANDLER_START", 1),
    ("PHP_OUTPUT_HANDLER_WRITE", 0),
    ("PHP_OUTPUT_HANDLER_FLUSH", 4),
    ("PHP_OUTPUT_HANDLER_CLEAN", 2),
    ("PHP_OUTPUT_HANDLER_FINAL", 8),
    ("PHP_OUTPUT_HANDLER_CONT", 0),
    ("PHP_OUTPUT_HANDLER_END", 8),
    ("PHP_OUTPUT_HANDLER_CLEANABLE", CLEANABLE),
    ("PHP_OUTPUT_HANDLER_FLUSHABLE", FLUSHABLE),
    ("PHP_OUTPUT_HANDLER_REMOVABLE", REMOVABLE),
    ("PHP_OUTPUT_HANDLER_STDFLAGS", STDFLAGS),
    ("PHP_OUTPUT_HANDLER_STARTED", STARTED),
    ("PHP_OUTPUT_HANDLER_DISABLED", DISABLED),
    ("PHP_OUTPUT_HANDLER_PROCESSED", PROCESSED),
];

#[derive(Default)]
pub(crate) struct Buffers {
    stack: Vec<Buffer>,
    running: Option<Running>,
    pub(crate) pending: Option<usize>,
    lock_broken: bool,
    finalizing: bool,
}

struct Running {
    index: usize,
}

struct Buffer {
    name: String,
    callback: Option<Callee>,
    bytes: Vec<u8>,
    chunk: usize,
    capacity: usize,
    settings: i64,
    started: bool,
    disabled: bool,
    processed: bool,
    produced_output: bool,
}

impl Buffer {
    fn new(name: String, callback: Option<Callee>, chunk: usize, flags: i64) -> Self {
        Self {
            name,
            callback,
            bytes: Vec::new(),
            chunk,
            capacity: aligned_capacity(chunk),
            settings: i64::from(flags as i32 & !0xf00f),
            started: false,
            disabled: false,
            processed: false,
            produced_output: false,
        }
    }

    fn append(&mut self, bytes: &[u8]) {
        let available = self.capacity.saturating_sub(self.bytes.len());
        if bytes.len() >= available {
            let needed = bytes.len() - available;
            self.capacity = self
                .capacity
                .saturating_add(aligned_capacity(self.chunk).max(aligned_capacity(needed)));
        }
        self.bytes.extend_from_slice(bytes);
    }

    fn status(&self, level: usize) -> Value {
        let kind = i64::from(self.callback.is_some());
        let mut flags = self.settings | kind;
        for (enabled, flag) in [
            (self.started, STARTED),
            (self.disabled, DISABLED),
            (self.processed, PROCESSED),
            (self.produced_output, PRODUCED_OUTPUT),
        ] {
            if enabled {
                flags |= flag;
            }
        }
        let mut status = PhpArray::new();
        for (key, value) in [
            ("name", Value::from(self.name.as_str())),
            ("type", Value::Int(kind)),
            ("flags", Value::Int(flags)),
            ("level", Value::Int(level as i64)),
            ("chunk_size", Value::Int(self.chunk as i64)),
            ("buffer_size", Value::Int(self.capacity as i64)),
            ("buffer_used", Value::Int(self.bytes.len() as i64)),
        ] {
            status.insert(ArrayKey::str(key), value);
        }
        Value::from(status)
    }
}

impl Buffers {
    pub(crate) fn len(&self) -> usize {
        self.stack.len()
    }

    /// Returns false when the bytes have reached the bottom of the stack.
    pub(crate) fn write(&mut self, depth: usize, bytes: &[u8]) -> bool {
        if let Some(running) = &self.running
            && let Some(buffer) = self.stack.get_mut(running.index)
            && !buffer.disabled
        {
            buffer.produced_output = true;
            buffer.append(bytes);
            return true;
        }
        let Some((index, buffer)) = self
            .stack
            .iter_mut()
            .take(depth)
            .enumerate()
            .rev()
            .find(|(_, buffer)| !buffer.disabled)
        else {
            return false;
        };
        // A deprecation emitted while a handler is disabled still counts as
        // handler output for PHP's next enabled buffer.
        if self.running.is_some() {
            buffer.produced_output = true;
        }
        buffer.append(bytes);
        if buffer.chunk > 0 && buffer.bytes.len() >= buffer.chunk {
            self.pending = Some(index);
        }
        true
    }
}

fn aligned_capacity(size: usize) -> usize {
    if size == 0 {
        DEFAULT_SIZE
    } else {
        size.saturating_add(CHUNK_ALIGNMENT - 1) & !(CHUNK_ALIGNMENT - 1)
    }
}

/// A fork keeps the buffers `ob_start()` opened before the snapshot, with
/// their handlers, but not what they held: output written before the
/// snapshot is not sent again.
impl crate::snapshot::ForkCopy for Buffers {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        Buffers {
            stack: self
                .stack
                .iter()
                .map(|buffer| Buffer {
                    name: buffer.name.clone(),
                    callback: buffer.callback.fork_copy(fork),
                    bytes: Vec::new(),
                    chunk: buffer.chunk,
                    capacity: buffer.capacity,
                    settings: buffer.settings,
                    started: buffer.started,
                    disabled: buffer.disabled,
                    processed: buffer.processed,
                    produced_output: false,
                })
                .collect(),
            running: None,
            pending: None,
            lock_broken: self.lock_broken,
            finalizing: false,
        }
    }
}
