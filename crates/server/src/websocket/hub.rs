//! Process-wide WebSocket connection and room registry (`Send + Sync`).
//!
//! Ordinary HTTP requests never touch the hub. Only connections upgraded to
//! WebSocket register here, and room fan-out clones refcounted `Bytes` payloads
//! into bounded per-connection channels without touching PHP values or locks
//! across `.await` points.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use bytes::Bytes;
use tokio::sync::{Notify, mpsc};

const SHARDS: usize = 64;

/// Outbound command queued for a connection's socket writer loop.
#[derive(Debug, Clone)]
pub(crate) enum OutboundCommand {
    Send { data: Bytes, binary: bool },
    Close { code: u16, reason: String },
}

impl OutboundCommand {
    fn payload_bytes(&self) -> usize {
        match self {
            Self::Send { data, .. } => data.len(),
            Self::Close { reason, .. } => reason.len(),
        }
    }
}

struct ConnectionEntry {
    tx: mpsc::Sender<OutboundCommand>,
    queued_bytes: Arc<AtomicUsize>,
    max_queue_bytes: usize,
    disconnect: Arc<Notify>,
    close_notify: Arc<Notify>,
    pending_close: Arc<Mutex<Option<(u16, String)>>>,
    context: RwLock<Vec<u8>>,
    rooms: RwLock<Vec<String>>,
    closed: AtomicBool,
}

pub(crate) struct RegisteredConnection {
    pub(crate) id: u64,
    pub(crate) id_str: String,
    pub(crate) rx_outbound: mpsc::Receiver<OutboundCommand>,
    pub(crate) queued_bytes: Arc<AtomicUsize>,
    pub(crate) disconnect: Arc<Notify>,
    pub(crate) close_notify: Arc<Notify>,
    pub(crate) pending_close: Arc<Mutex<Option<(u16, String)>>>,
}

#[derive(Clone)]
struct RoomSubscriber {
    id: u64,
    tx: mpsc::Sender<OutboundCommand>,
    queued_bytes: Arc<AtomicUsize>,
    max_queue_bytes: usize,
    disconnect: Arc<Notify>,
}

/// Shared registry of active WebSocket connections and room subscriptions
/// across all worker threads in one server process.
pub struct WebSocketHub {
    next_id: AtomicU64,
    send_queue_len: usize,
    send_queue_bytes: usize,
    connections: Vec<RwLock<HashMap<u64, Arc<ConnectionEntry>>>>,
    rooms: Vec<RwLock<HashMap<String, Vec<RoomSubscriber>>>>,
}

impl Default for WebSocketHub {
    fn default() -> Self {
        Self::new(64, 1024 * 1024)
    }
}

impl WebSocketHub {
    pub fn new(send_queue_len: usize, send_queue_bytes: usize) -> Self {
        let connections = (0..SHARDS).map(|_| RwLock::new(HashMap::new())).collect();
        let rooms = (0..SHARDS).map(|_| RwLock::new(HashMap::new())).collect();
        Self {
            next_id: AtomicU64::new(1),
            send_queue_len: send_queue_len.max(1),
            send_queue_bytes: send_queue_bytes.max(1024),
            connections,
            rooms,
        }
    }

    pub(crate) fn register(
        &self,
        initial_context: Vec<u8>,
        initial_rooms: Vec<String>,
    ) -> RegisteredConnection {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id_str = format!("ws-{id}");
        let (tx, rx) = mpsc::channel(self.send_queue_len);
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let disconnect = Arc::new(Notify::new());
        let close_notify = Arc::new(Notify::new());
        let pending_close = Arc::new(Mutex::new(None));
        let entry = Arc::new(ConnectionEntry {
            tx,
            queued_bytes: queued_bytes.clone(),
            max_queue_bytes: self.send_queue_bytes,
            disconnect: disconnect.clone(),
            close_notify: close_notify.clone(),
            pending_close: pending_close.clone(),
            context: RwLock::new(initial_context),
            rooms: RwLock::new(Vec::new()),
            closed: AtomicBool::new(false),
        });
        let shard = shard_for_id(id);
        if let Ok(mut map) = self.connections[shard].write() {
            map.insert(id, entry);
        }
        for room in initial_rooms {
            if !room.is_empty() {
                self.join(&id_str, &room);
            }
        }
        RegisteredConnection {
            id,
            id_str,
            rx_outbound: rx,
            queued_bytes,
            disconnect,
            close_notify,
            pending_close,
        }
    }

    pub(crate) fn remove(&self, id: u64) {
        let shard = shard_for_id(id);
        let entry = self.connections[shard]
            .write()
            .ok()
            .and_then(|mut map| map.remove(&id));
        let Some(entry) = entry else { return };
        entry.closed.store(true, Ordering::Release);
        let rooms = entry
            .rooms
            .write()
            .ok()
            .map(|mut rooms| std::mem::take(&mut *rooms))
            .unwrap_or_default();
        for room in rooms {
            let room_shard = shard_for_name(&room);
            if let Ok(mut map) = self.rooms[room_shard].write()
                && let Some(subscribers) = map.get_mut(&room)
            {
                subscribers.retain(|sub| sub.id != id);
                if subscribers.is_empty() {
                    map.remove(&room);
                }
            }
        }
    }

    /// Queue a WebSocket message to a single connection. Returns `false` if the
    /// connection is gone, text payload is invalid UTF-8, or its outbound queue
    /// overflowed.
    pub fn send(&self, connection_id: &str, data: &[u8], binary: bool) -> bool {
        if !binary && std::str::from_utf8(data).is_err() {
            return false;
        }
        let Some(id) = parse_connection_id(connection_id) else {
            return false;
        };
        let Some(entry) = self.connection_entry(id) else {
            return false;
        };
        if entry.closed.load(Ordering::Acquire) {
            return false;
        }
        let cmd = OutboundCommand::Send {
            data: Bytes::copy_from_slice(data),
            binary,
        };
        try_enqueue(
            &entry.tx,
            &entry.queued_bytes,
            entry.max_queue_bytes,
            &entry.disconnect,
            cmd,
        )
    }

    /// Broadcast a message to all connections in `room`, optionally skipping
    /// `exclude_connection_id`. Slow subscribers whose queue fills are evicted
    /// immediately rather than stalling the room.
    pub fn broadcast(
        &self,
        room: &str,
        data: &[u8],
        binary: bool,
        exclude_connection_id: Option<&str>,
    ) -> usize {
        if !binary && std::str::from_utf8(data).is_err() {
            return 0;
        }
        let exclude = exclude_connection_id.and_then(parse_connection_id);
        let room_shard = shard_for_name(room);
        let subscribers = {
            let Ok(map) = self.rooms[room_shard].read() else {
                return 0;
            };
            let Some(subs) = map.get(room) else {
                return 0;
            };
            subs.clone()
        };
        if subscribers.is_empty() {
            return 0;
        }
        let payload = Bytes::copy_from_slice(data);
        let mut delivered = 0usize;
        for sub in subscribers {
            if Some(sub.id) == exclude {
                continue;
            }
            let cmd = OutboundCommand::Send {
                data: payload.clone(),
                binary,
            };
            if try_enqueue(
                &sub.tx,
                &sub.queued_bytes,
                sub.max_queue_bytes,
                &sub.disconnect,
                cmd,
            ) {
                delivered += 1;
            }
        }
        delivered
    }

    pub fn join(&self, connection_id: &str, room: &str) -> bool {
        if room.is_empty() {
            return false;
        }
        let Some(id) = parse_connection_id(connection_id) else {
            return false;
        };
        let Some(entry) = self.connection_entry(id) else {
            return false;
        };
        {
            let Ok(mut rooms) = entry.rooms.write() else {
                return false;
            };
            if rooms.iter().any(|existing| existing == room) {
                return true;
            }
            rooms.push(room.to_string());
        }
        let room_shard = shard_for_name(room);
        if let Ok(mut map) = self.rooms[room_shard].write() {
            let subs = map.entry(room.to_string()).or_default();
            if !subs.iter().any(|sub| sub.id == id) {
                subs.push(RoomSubscriber {
                    id,
                    tx: entry.tx.clone(),
                    queued_bytes: entry.queued_bytes.clone(),
                    max_queue_bytes: entry.max_queue_bytes,
                    disconnect: entry.disconnect.clone(),
                });
            }
        }
        true
    }

    pub fn leave(&self, connection_id: &str, room: &str) -> bool {
        let Some(id) = parse_connection_id(connection_id) else {
            return false;
        };
        let Some(entry) = self.connection_entry(id) else {
            return false;
        };
        let removed = entry
            .rooms
            .write()
            .ok()
            .map(|mut rooms| {
                let before = rooms.len();
                rooms.retain(|existing| existing != room);
                rooms.len() != before
            })
            .unwrap_or(false);
        if removed {
            let room_shard = shard_for_name(room);
            if let Ok(mut map) = self.rooms[room_shard].write()
                && let Some(subs) = map.get_mut(room)
            {
                subs.retain(|sub| sub.id != id);
                if subs.is_empty() {
                    map.remove(room);
                }
            }
        }
        removed
    }

    pub fn rooms(&self, connection_id: &str) -> Vec<String> {
        let Some(id) = parse_connection_id(connection_id) else {
            return Vec::new();
        };
        let Some(entry) = self.connection_entry(id) else {
            return Vec::new();
        };
        entry
            .rooms
            .read()
            .map(|rooms| rooms.clone())
            .unwrap_or_default()
    }

    pub fn room_count(&self, room: &str) -> usize {
        let room_shard = shard_for_name(room);
        self.rooms[room_shard]
            .read()
            .ok()
            .and_then(|map| map.get(room).map(Vec::len))
            .unwrap_or(0)
    }

    pub fn connections(&self, room: &str) -> Vec<String> {
        let room_shard = shard_for_name(room);
        self.rooms[room_shard]
            .read()
            .ok()
            .and_then(|map| {
                map.get(room)
                    .map(|subs| subs.iter().map(|sub| format!("ws-{}", sub.id)).collect())
            })
            .unwrap_or_default()
    }

    pub fn close(&self, connection_id: &str, code: u16, reason: &str) -> bool {
        let Some(id) = parse_connection_id(connection_id) else {
            return false;
        };
        let Some(entry) = self.connection_entry(id) else {
            return false;
        };
        if entry.closed.swap(true, Ordering::AcqRel) {
            return false;
        }
        let cmd = OutboundCommand::Close {
            code,
            reason: reason.to_string(),
        };
        if let Err(mpsc::error::TrySendError::Full(OutboundCommand::Close { code, reason })) =
            entry.tx.try_send(cmd)
        {
            // Queue is backed up; wake the socket loop directly so close() cannot hang.
            if let Ok(mut pending) = entry.pending_close.lock() {
                *pending = Some((code, reason));
            }
            entry.close_notify.notify_one();
        }
        true
    }

    pub fn context(&self, connection_id: &str) -> Option<Vec<u8>> {
        let id = parse_connection_id(connection_id)?;
        let entry = self.connection_entry(id)?;
        entry.context.read().ok().map(|bytes| bytes.clone())
    }

    pub fn set_context(&self, connection_id: &str, bytes: Vec<u8>) -> bool {
        let Some(id) = parse_connection_id(connection_id) else {
            return false;
        };
        let Some(entry) = self.connection_entry(id) else {
            return false;
        };
        if let Ok(mut guard) = entry.context.write() {
            *guard = bytes;
            true
        } else {
            false
        }
    }

    fn connection_entry(&self, id: u64) -> Option<Arc<ConnectionEntry>> {
        let shard = shard_for_id(id);
        self.connections[shard]
            .read()
            .ok()
            .and_then(|map| map.get(&id).cloned())
    }
}

fn try_enqueue(
    tx: &mpsc::Sender<OutboundCommand>,
    queued_bytes: &AtomicUsize,
    max_queue_bytes: usize,
    disconnect: &Notify,
    cmd: OutboundCommand,
) -> bool {
    let size = cmd.payload_bytes();
    let prev = queued_bytes.fetch_add(size, Ordering::AcqRel);
    if prev.saturating_add(size) > max_queue_bytes {
        queued_bytes.fetch_sub(size, Ordering::AcqRel);
        disconnect.notify_one();
        return false;
    }
    match tx.try_send(cmd) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            queued_bytes.fetch_sub(size, Ordering::AcqRel);
            disconnect.notify_one();
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            queued_bytes.fetch_sub(size, Ordering::AcqRel);
            false
        }
    }
}

fn parse_connection_id(id: &str) -> Option<u64> {
    id.strip_prefix("ws-")?.parse().ok()
}

fn shard_for_id(id: u64) -> usize {
    (id as usize) % SHARDS
}

fn shard_for_name(name: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash as usize) % SHARDS
}
