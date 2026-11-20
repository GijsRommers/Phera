//! `source -i 4 -f file:///app.php -b 3 -e 5` returns lines 3 through 5.

use super::protocol::Command;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub(super) async fn source(command: &Command, fallback: &str) -> String {
    let file = command.get(b'f').unwrap_or(fallback);
    let Some(path) = url::Url::parse(file)
        .ok()
        .and_then(|uri| uri.to_file_path().ok())
    else {
        return command.error(100, "could not open source");
    };
    let Ok(bytes) = tokio::fs::read(path).await else {
        return command.error(100, "could not open source");
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
