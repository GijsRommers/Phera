//! `mail.log` records each attempted message, and `mail.add_x_header` adds
//! an originating-script header before the message goes to sendmail.

use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use phera_extension_api::{Context, NativeError, Value, WorkingDirectory};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixDatagram;

use super::Message;

pub(super) async fn write(
    context: &mut dyn Context,
    function: &str,
    message: &Message,
) -> Result<(), NativeError> {
    let Some(log) = context.ini_value(b"mail.log").filter(|log| !log.is_empty()) else {
        return Ok(());
    };
    let (source, line) = context.source_location().unwrap_or_default();
    let mut text = format!("mail() on [{source}:{line}]: To: ").into_bytes();
    text.extend_from_slice(&message.to);
    text.extend_from_slice(b" -- Headers: ");
    text.extend_from_slice(&message.headers);
    text.extend_from_slice(b" -- Subject: ");
    text.extend_from_slice(&message.subject);
    if !message.headers.is_empty() {
        for byte in &mut text {
            if matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    if log.as_bytes() == b"syslog" {
        let path = if cfg!(target_os = "macos") {
            "/var/run/syslog"
        } else {
            "/dev/log"
        };
        if let Ok(socket) = UnixDatagram::unbound()
            && socket.connect(path).is_ok()
        {
            let mut record = format!("<13>php[{}]: ", std::process::id()).into_bytes();
            record.extend_from_slice(&text);
            let _ = socket.send(&record).await;
        }
        return Ok(());
    }
    let date = context
        .call(&Value::from("date"), vec![Value::from("d-M-Y H:i:s e")])
        .await?;
    let mut record = Vec::new();
    record.extend_from_slice(b"[");
    if let Some(date) = date.as_str_bytes() {
        record.extend_from_slice(date);
    }
    record.extend_from_slice(b"] ");
    record.extend_from_slice(&text);
    record.push(b'\n');
    let path = Path::new(OsStr::from_bytes(log.as_bytes()));
    let result = append(context, path, &record).await;
    if let Err(error) = result {
        context.warning(format!(
            "{function}({}): Failed to open stream: {}",
            path.display(),
            crate::command::reason(&error)
        ));
    }
    Ok(())
}

async fn append(context: &mut dyn Context, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let path = context
        .state()
        .get_or_default::<WorkingDirectory>()
        .resolve(path)?;
    tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?
        .write_all(bytes)
        .await
}

pub(super) async fn add_origin_header(
    context: &dyn Context,
    message: &mut Message,
    separator: &[u8],
) {
    let uid = if let Some(path) = context.entry_script_path() {
        tokio::fs::metadata(path).await.map(|file| file.uid()).ok()
    } else {
        None
    };
    let uid = uid.unwrap_or_else(|| nix::unistd::getuid().as_raw());
    let (source, _) = context.source_location().unwrap_or_default();
    let basename = source.rsplit('/').next().unwrap_or_default();
    let mut header = format!("X-PHP-Originating-Script: {uid}:{basename}").into_bytes();
    if !message.headers.is_empty() {
        header.extend_from_slice(separator);
        header.append(&mut message.headers);
    }
    message.headers = header;
}
