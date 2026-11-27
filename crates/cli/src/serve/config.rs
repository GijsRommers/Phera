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

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Settings {
    pub(super) preload: bool,
    /// Optional `Phera\Http\Router` script mounted ahead of static files and
    /// the front controller (`[server] routes = "routes.php"`).
    pub(super) routes: Option<PathBuf>,
    /// The paths handled as requests before the snapshot (see `--warmup`).
    pub(super) warmup: Vec<String>,
    pub(super) max_connections: Option<usize>,
    pub(super) backlog: Option<i32>,
    pub(super) idle_timeout: Option<Duration>,
    pub(super) keepalive_shed_at: Option<usize>,
    pub(super) max_active_requests: Option<usize>,
    pub(super) websocket: WebSocketSettings,
    pub(super) path: Option<PathBuf>,
    pub(super) table: toml::Table,
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
    let (table, config_path) = crate::debug_settings::read_table(project)?;
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
    let routes = match server.and_then(|server| server.get("routes")) {
        Some(toml::Value::String(entry)) if !entry.trim().is_empty() => {
            let candidate = PathBuf::from(entry.trim());
            Some(if candidate.is_absolute() {
                candidate
            } else {
                project.join(candidate)
            })
        }
        Some(_) => {
            return Err(format!(
                "{}: server.routes must be a non-empty file path string",
                path.display()
            ));
        }
        None => None,
    };
    let max_connections = match server.and_then(|server| server.get("max_connections")) {
        Some(value) => Some(server_positive_usize(&path, "max_connections", value)?),
        None => None,
    };
    let backlog = match server.and_then(|server| server.get("backlog")) {
        Some(value) => {
            let n = server_positive_usize(&path, "backlog", value)?;
            Some(
                i32::try_from(n)
                    .map_err(|_| format!("{}: server.backlog is too large: {n}", path.display()))?,
            )
        }
        None => None,
    };
    let idle_timeout = match server.and_then(|server| server.get("idle_timeout")) {
        Some(value) => {
            let seconds = server_positive_usize(&path, "idle_timeout", value)?;
            Some(Duration::from_secs(seconds as u64))
        }
        None => None,
    };
    let keepalive_shed_at = match server.and_then(|server| server.get("keepalive_shed_at")) {
        Some(value) => Some(server_positive_usize(&path, "keepalive_shed_at", value)?),
        None => None,
    };
    let max_active_requests = match server.and_then(|server| server.get("max_active_requests")) {
        Some(value) => Some(server_positive_usize(&path, "max_active_requests", value)?),
        None => None,
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
        routes,
        warmup,
        max_connections,
        backlog,
        idle_timeout,
        keepalive_shed_at,
        max_active_requests,
        websocket,
        path: config_path,
        table,
    })
}

fn server_positive_usize(path: &Path, key: &str, value: &toml::Value) -> Result<usize, String> {
    match value.as_integer().and_then(|n| usize::try_from(n).ok()) {
        Some(n) if n > 0 => Ok(n),
        _ => Err(format!(
            "{}: server.{key} must be a positive integer",
            path.display()
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_websocket_toml_settings() {
        let dir = std::env::temp_dir().join(format!("phera-ws-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("phera.toml"),
            r#"
[server.websocket]
max_connections = 1234
max_message_size = 4096
idle_timeout = 0
ping_interval = 15
send_queue = 32
send_queue_bytes = 65536
allowed_origins = ["https://app.example.test", "http://localhost:3000"]
"#,
        )
        .expect("write");

        let parsed = read(&dir).expect("valid config");
        assert_eq!(parsed.websocket.max_connections, Some(1234));
        assert_eq!(parsed.websocket.max_message_size, Some(4096));
        assert_eq!(parsed.websocket.idle_timeout, Some(Duration::ZERO));
        assert_eq!(
            parsed.websocket.ping_interval,
            Some(Duration::from_secs(15))
        );
        assert_eq!(parsed.websocket.send_queue, Some(32));
        assert_eq!(parsed.websocket.send_queue_bytes, Some(65536));
        assert_eq!(
            parsed.websocket.allowed_origins.as_deref(),
            Some(
                [
                    "https://app.example.test".to_owned(),
                    "http://localhost:3000".to_owned()
                ]
                .as_slice()
            )
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
