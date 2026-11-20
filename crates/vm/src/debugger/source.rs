//! `source -i 4 -f file:///app.php -b 3 -e 5` returns lines 3 through 5.
//! A script's `eval()` is available under a session-local `dbgp://` URI.

use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_bytecode::CompiledUnit;

use super::protocol::{Command, file_uri};

#[derive(Default)]
pub(crate) struct Sources {
    units: HashMap<usize, String>,
    evaluated: Vec<Vec<u8>>,
}

impl Sources {
    pub(crate) fn remember(&mut self, unit: &CompiledUnit, bytes: Vec<u8>) {
        self.evaluated.push(bytes);
        self.units.insert(
            std::ptr::from_ref(unit) as usize,
            format!("dbgp://{}", self.evaluated.len()),
        );
    }

    pub(super) fn uri(&self, unit: &CompiledUnit) -> String {
        self.units
            .get(&(std::ptr::from_ref(unit) as usize))
            .cloned()
            .unwrap_or_else(|| file_uri(&unit.file))
    }

    pub(super) async fn command(&self, command: &Command, fallback: &str) -> String {
        let file = command.get(b'f').unwrap_or(fallback);
        let loaded;
        let bytes = if let Some(id) = file.strip_prefix("dbgp://") {
            let index = id.parse::<usize>().ok().and_then(|id| id.checked_sub(1));
            let Some(bytes) = index.and_then(|index| self.evaluated.get(index)) else {
                return command.error(100, "could not open source");
            };
            bytes.as_slice()
        } else {
            let Some(path) = url::Url::parse(file)
                .ok()
                .and_then(|uri| uri.to_file_path().ok())
            else {
                return command.error(100, "could not open source");
            };
            loaded = match tokio::fs::read(path).await {
                Ok(bytes) => bytes,
                Err(_) => return command.error(100, "could not open source"),
            };
            &loaded
        };
        let first = command.number::<usize>(b'b').unwrap_or(1).max(1);
        let last = command.number::<usize>(b'e').unwrap_or(usize::MAX);
        let range: Vec<u8> = bytes
            .split_inclusive(|byte| *byte == b'\n')
            .skip(first - 1)
            .take(last.saturating_add(1).saturating_sub(first))
            .flatten()
            .copied()
            .collect();
        command.response(r#" encoding="base64""#, &STANDARD.encode(range))
    }
}
