//! What a project sets for `phera serve` in its `phera.toml`: the
//! `--preload` option, the warm-up requests of `--warmup`, and
//! `[server.websocket]` limits.
//!
//! ```toml
//! [server]
//! preload = true
//! warmup = ["/api/posts?page=1", "/"]
//!
//! [server.websocket]
//! max_connections = 5000
//! idle_timeout = 90
//! ```

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Settings {
    pub(super) preload: bool,
    /// The paths handled as requests before the snapshot (see `--warmup`).
    pub(super) warmup: Vec<String>,
    pub(super) websocket: WebSocketSettings,
    pub(super) path: Option<PathBuf>,
}

#[derive(Default)]
pub(super) struct WebSocketSettings {
    pub(super) max_connections: Option<usize>,
    pub(super) max_message_size: Option<usize>,
    pub(super) idle_timeout: Option<Duration>,
    pub(super) ping_interval: Option<Duration>,
    pub(super) send_queue: Option<usize>,
    pub(super) send_queue_bytes: Option<usize>,
    pub(super) allowed_origins: Option<Vec<String>>,
}

pub(super) fn project_root(document_root: &Path) -> &Path {
    let is_project = |path: &Path| {
        path.join("composer.json").is_file()
            || path.join("phera.toml").is_file()
            || path.join("vendor/composer").is_dir()
    };
    if is_project(document_root) {
        document_root
    } else {
        document_root
            .parent()
            .filter(|path| is_project(path))
            .unwrap_or(document_root)
    }
}

pub(super) fn read(project: &Path) -> Result<Settings, String> {
    let path = project.join("phera.toml");
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let table: toml::Table = source
        .parse()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let server = table.get("server");
    let preload = server.and_then(|server| server.get("preload"));
    let preload = match preload {
        Some(toml::Value::Boolean(enabled)) => *enabled,
        Some(_) => {
            return Err(format!(
                "{}: server.preload must be true or false",
                path.display()
            ));
        }
        None => false,
    };
    let warmup = match server.and_then(|server| server.get("warmup")) {
        Some(toml::Value::Array(paths)) => paths
            .iter()
            .map(|entry| match entry {
                toml::Value::String(entry) if crate::arguments::is_warmup_path(entry) => {
                    Ok(entry.clone())
                }
                _ => Err(format!(
                    "{}: server.warmup must be a list of paths like \"/api/posts?page=1\", without spaces",
                    path.display()
                )),
            })
            .collect::<Result<Vec<String>, String>>()?,
        Some(_) => {
            return Err(format!(
                "{}: server.warmup must be a list of paths",
                path.display()
            ));
        }
        None => Vec::new(),
    };
    let websocket = match server.and_then(|server| server.get("websocket")) {
        Some(toml::Value::Table(ws)) => read_websocket(&path, ws)?,
        Some(_) => {
            return Err(format!(
                "{}: server.websocket must be a table",
                path.display()
            ));
        }
        None => WebSocketSettings::default(),
    };
    Ok(Settings {
        preload,
        warmup,
        websocket,
        path: path.canonicalize().ok(),
    })
}

fn read_websocket(
    path: &Path,
    ws: &toml::map::Map<String, toml::Value>,
) -> Result<WebSocketSettings, String> {
    let mut settings = WebSocketSettings::default();
    if let Some(value) = ws.get("max_connections") {
        settings.max_connections = Some(positive_usize(path, "max_connections", value)?);
    }
    if let Some(value) = ws.get("max_message_size") {
        settings.max_message_size = Some(positive_usize(path, "max_message_size", value)?);
    }
    if let Some(value) = ws.get("idle_timeout") {
        settings.idle_timeout = Some(Duration::from_secs(non_negative_u64(
            path,
            "idle_timeout",
            value,
        )?));
    }
    if let Some(value) = ws.get("ping_interval") {
        settings.ping_interval = Some(Duration::from_secs(non_negative_u64(
            path,
            "ping_interval",
            value,
        )?));
    }
    if let Some(value) = ws.get("send_queue") {
        settings.send_queue = Some(positive_usize(path, "send_queue", value)?);
    }
    if let Some(value) = ws.get("send_queue_bytes") {
        settings.send_queue_bytes = Some(positive_usize(path, "send_queue_bytes", value)?);
    }
    if let Some(value) = ws.get("allowed_origins") {
        let toml::Value::Array(items) = value else {
            return Err(format!(
                "{}: server.websocket.allowed_origins must be a list of origin strings",
                path.display()
            ));
        };
        let mut origins = Vec::with_capacity(items.len());
        for item in items {
            let toml::Value::String(origin) = item else {
                return Err(format!(
                    "{}: server.websocket.allowed_origins entries must be strings",
                    path.display()
                ));
            };
            let trimmed = origin.trim();
            if trimmed.is_empty() {
                return Err(format!(
                    "{}: server.websocket.allowed_origins must not contain empty strings",
                    path.display()
                ));
            }
            origins.push(trimmed.to_owned());
        }
        settings.allowed_origins = Some(origins);
    }
    Ok(settings)
}

fn positive_usize(path: &Path, key: &str, value: &toml::Value) -> Result<usize, String> {
    match value.as_integer().and_then(|n| usize::try_from(n).ok()) {
        Some(n) if n > 0 => Ok(n),
        _ => Err(format!(
            "{}: server.websocket.{key} must be a positive integer",
            path.display()
        )),
    }
}

fn non_negative_u64(path: &Path, key: &str, value: &toml::Value) -> Result<u64, String> {
    match value.as_integer().and_then(|n| u64::try_from(n).ok()) {
        Some(n) => Ok(n),
        None => Err(format!(
            "{}: server.websocket.{key} must be a non-negative integer",
            path.display()
        )),
    }
}
