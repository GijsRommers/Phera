//! `session_start()` locks `sess_<id>` until the session closes.
//!
//! A process-wide async mutex queues requests for one path across workers. While
//! waiting for a PHP-FPM process to release the advisory lock, a request tries
//! the lock on the blocking pool and sleeps asynchronously between attempts.
//! Cancelling a request drops its mutex guard and descriptor, including during
//! acquisition. A close that is already writing owns both until its write ends.

use std::collections::HashMap;
use std::fs::{self, File, FileTimes, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, SystemTime};

use phera_ext_streams::file_io::{blocking, file_is_owned, open_private_file, resolve, same_file};
use phera_extension_api::{Context, NativeError, PhpString};

use crate::id;

const PREFIX: &str = "sess_";

type Gate = tokio::sync::Mutex<()>;
type Gates = HashMap<PathBuf, Weak<Gate>>;
static GATES: OnceLock<Mutex<Gates>> = OnceLock::new();

#[derive(Clone)]
pub struct Directory {
    root: PathBuf,
    depth: usize,
    mode: u32,
}

pub struct LockedFile {
    file: File,
    path: PathBuf,
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

#[derive(Debug)]
pub enum FileError {
    Id,
    Path,
    Owner,
    Open(PathBuf, io::Error),
    Io(io::Error),
}

impl Directory {
    pub fn from_setting(context: &mut dyn Context, setting: &PhpString) -> Result<Self, String> {
        let mut parts = setting.as_bytes().splitn(3, |byte| *byte == b';');
        let first = parts.next().unwrap_or_default();
        let second = parts.next();
        let third = parts.next();
        let (raw, depth, mode) = match (second, third) {
            (Some(mode), Some(raw)) => {
                let mode = parse(mode, 8)
                    .filter(|mode| *mode <= 0o7777)
                    .ok_or("The second parameter in session.save_path is invalid")?;
                (raw, depth(first)?, mode as u32)
            }
            (Some(raw), None) => (raw, depth(first)?, 0o600),
            _ => (first, 0, 0o600),
        };
        let root = if setting.is_empty() {
            std::env::temp_dir()
        } else {
            resolve(context, raw).map_err(|error| error.to_string())?
        };
        Ok(Self { root, depth, mode })
    }

    fn path(&self, id: &PhpString) -> Result<PathBuf, FileError> {
        let id = id.as_bytes();
        if !id::valid(id) {
            return Err(FileError::Id);
        }
        if id.len() <= self.depth {
            return Err(FileError::Path);
        }
        let mut path = self.root.clone();
        for byte in &id[..self.depth] {
            path.push(char::from(*byte).to_string());
        }
        path.push(format!("{PREFIX}{}", String::from_utf8_lossy(id)));
        Ok(path)
    }

    pub async fn known(&self, id: &PhpString) -> Result<bool, NativeError> {
        let Ok(path) = self.path(id) else {
            return Ok(false);
        };
        blocking(move || path.is_file()).await
    }

    pub async fn lock(&self, id: &PhpString) -> Result<Result<LockedFile, FileError>, NativeError> {
        let path = match self.path(id) {
            Ok(path) => path,
            Err(error) => return Ok(Err(error)),
        };
        let path = blocking(
            move || match path.parent().and_then(|dir| dir.canonicalize().ok()) {
                Some(root) => path
                    .file_name()
                    .map_or_else(|| path.clone(), |name| root.join(name)),
                None => path,
            },
        )
        .await?;
        let guard = gate(&path).lock_owned().await;
        let mode = self.mode;
        loop {
            let opened = try_open_locked(path.clone(), mode).await?;
            match opened {
                Ok(Some(file)) => {
                    return Ok(Ok(LockedFile {
                        file,
                        path,
                        _guard: guard,
                    }));
                }
                Ok(None) => tokio::time::sleep(Duration::from_millis(10)).await,
                Err(error) => return Ok(Err(error)),
            }
        }
    }

    pub async fn gc(&self, lifetime: i64) -> Result<Option<i64>, NativeError> {
        if self.depth != 0 {
            return Ok(None);
        }
        let root = self.root.clone();
        blocking(move || {
            let now = SystemTime::now();
            let mut removed = 0i64;
            for entry in fs::read_dir(root).ok()?.flatten() {
                let name = entry.file_name();
                let Some(id) = name.as_encoded_bytes().strip_prefix(PREFIX.as_bytes()) else {
                    continue;
                };
                if !id::valid(id) {
                    continue;
                }
                // Take an advisory lock without waiting. An active session can have
                // an old mtime while a different request is still processing it.
                let path = entry.path();
                let attempt = open_private_file(&path, 0, false)
                    .and_then(|file| file.try_lock().map(|()| file).map_err(io::Error::from));
                if let Ok(file) = attempt
                    && same_file(&file, &path)
                    && let Ok(mtime) = file.metadata().and_then(|info| info.modified())
                    && let Ok(age) = now.duration_since(mtime)
                    && (lifetime < 0 || age.as_secs() > lifetime as u64)
                    && fs::remove_file(path).is_ok()
                {
                    removed += 1;
                }
            }
            Some(removed)
        })
        .await
    }
}

async fn try_open_locked(
    path: PathBuf,
    mode: u32,
) -> Result<Result<Option<File>, FileError>, NativeError> {
    blocking(move || {
        let file = open_private_file(&path, mode, true)
            .map_err(|error| FileError::Open(path.clone(), error))?;
        if !file_is_owned(&file).map_err(FileError::Io)? {
            return Err(FileError::Owner);
        }
        match file.try_lock() {
            Ok(()) if same_file(&file, &path) => Ok(Some(file)),
            Ok(()) => Ok(None),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(FileError::Io(error)),
        }
    })
    .await
}

fn depth(bytes: &[u8]) -> Result<usize, &'static str> {
    parse(bytes, 10)
        .and_then(|depth| usize::try_from(depth).ok())
        .ok_or("The first parameter in session.save_path is invalid")
}

fn parse(bytes: &[u8], radix: u32) -> Option<u64> {
    let bytes = bytes.trim_ascii_start();
    let bytes = bytes.strip_prefix(b"+").unwrap_or(bytes);
    bytes
        .iter()
        .map_while(|byte| char::from(*byte).to_digit(radix))
        .try_fold(0u64, |number, digit| {
            number
                .checked_mul(u64::from(radix))?
                .checked_add(u64::from(digit))
        })
}

fn gate(path: &Path) -> Arc<Gate> {
    let gates = GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = gates
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = gates.get(path).and_then(Weak::upgrade) {
        return existing;
    }
    if gates.len() == gates.capacity() {
        gates.retain(|_, gate| gate.strong_count() != 0);
    }
    let gate = Arc::new(Gate::new(()));
    gates.insert(path.to_path_buf(), Arc::downgrade(&gate));
    gate
}

impl LockedFile {
    pub async fn read(self) -> Result<(Self, io::Result<Vec<u8>>), NativeError> {
        blocking(move || {
            let mut file = self;
            let result = (|| {
                file.file.seek(SeekFrom::Start(0))?;
                let mut data = Vec::new();
                let mut chunk = [0; 8192];
                loop {
                    let count = file.file.read(&mut chunk)?;
                    if count == 0 {
                        return Ok(data);
                    }
                    data.extend_from_slice(&chunk[..count]);
                }
            })();
            (file, result)
        })
        .await
    }

    pub async fn write(
        self,
        data: Vec<u8>,
        unchanged: bool,
    ) -> Result<(Self, io::Result<()>), NativeError> {
        blocking(move || {
            let mut file = self;
            let result = (|| {
                if unchanged {
                    let now = SystemTime::now();
                    let times = FileTimes::new().set_accessed(now).set_modified(now);
                    if file.file.set_times(times).is_ok() {
                        return Ok(());
                    }
                }
                file.file.seek(SeekFrom::Start(0))?;
                file.file.write_all(&data)?;
                file.file.set_len(data.len() as u64)
            })();
            (file, result)
        })
        .await
    }

    pub async fn destroy(self) -> Result<bool, NativeError> {
        blocking(move || {
            let result = match fs::remove_file(&self.path) {
                Ok(()) => true,
                Err(error) => error.kind() == io::ErrorKind::NotFound,
            };
            drop(self);
            result
        })
        .await
    }
}
