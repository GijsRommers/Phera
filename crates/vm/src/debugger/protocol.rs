//! `status -i 4\0` receives a length-prefixed DBGp XML response.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use quick_xml::escape::escape;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub(super) const NAMESPACES: &str =
    r#"xmlns="urn:debugger_protocol_v1" xmlns:xdebug="https://xdebug.org/dbgp/xdebug""#;
const MAX_COMMAND_SIZE: usize = 1024 * 1024;

pub(super) struct Command {
    pub(super) name: String,
    options: BTreeMap<u8, String>,
    payload: Option<Vec<u8>>,
}

impl Command {
    fn parse(input: &str) -> Option<Self> {
        let (name, mut rest) = input.split_once(' ')?;
        let mut command = Self {
            name: name.to_owned(),
            options: BTreeMap::new(),
            payload: None,
        };
        while !rest.trim_start().is_empty() {
            rest = rest.trim_start();
            if let Some(encoded) = rest.strip_prefix("-- ") {
                command.payload = Some(STANDARD.decode(encoded.trim()).ok()?);
                break;
            }
            let [b'-', flag, b' ', ..] = rest.as_bytes() else {
                return None;
            };
            rest = rest.get(3..)?.trim_start();
            let value = if let Some(quoted) = rest.strip_prefix('"') {
                let mut value = String::new();
                let mut escaped = false;
                let mut end = None;
                for (index, ch) in quoted.char_indices() {
                    match (escaped, ch) {
                        (false, '\\') => escaped = true,
                        (false, '"') => {
                            end = Some(index);
                            break;
                        }
                        (true, '0') => {
                            value.push('\0');
                            escaped = false;
                        }
                        _ => {
                            value.push(ch);
                            escaped = false;
                        }
                    }
                }
                rest = quoted.get(end? + 1..)?;
                value
            } else {
                let end = rest.find(' ').unwrap_or(rest.len());
                let (value, tail) = rest.split_at(end);
                rest = tail;
                value.to_owned()
            };
            if command.options.insert(*flag, value).is_some() {
                return None;
            }
        }
        command.get(b'i')?;
        Some(command)
    }

    pub(super) fn get(&self, flag: u8) -> Option<&str> {
        self.options.get(&flag).map(String::as_str)
    }

    pub(super) fn number<T: std::str::FromStr>(&self, flag: u8) -> Option<T> {
        self.get(flag)?.parse().ok()
    }

    pub(super) fn data(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }

    pub(super) fn response(&self, attrs: &str, body: &str) -> String {
        let name = escape(&self.name);
        let id = escape(self.get(b'i').unwrap_or_default());
        format!(
            r#"<response {NAMESPACES} command="{name}" transaction_id="{id}"{attrs}>{body}</response>"#
        )
    }

    pub(super) fn error(&self, code: u16, message: &str) -> String {
        self.error_response(code, message, "")
    }

    pub(super) fn error_with_status(&self, code: u16, message: &str, status: &str) -> String {
        self.error_response(code, message, &format!(r#" status="{status}" reason="ok""#))
    }

    fn error_response(&self, code: u16, message: &str, attrs: &str) -> String {
        self.response(
            attrs,
            &format!(
                r#"<error code="{code}"><message>{}</message></error>"#,
                escape(message)
            ),
        )
    }
}

/// A bounded command buffer shared by the waiting and running states.
pub(super) struct Wire {
    stream: TcpStream,
    input: Vec<u8>,
}

impl Wire {
    pub(super) fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            input: Vec::new(),
        }
    }

    fn next_command(&mut self) -> io::Result<Option<Command>> {
        let Some(end) = self.input.iter().position(|byte| *byte == 0) else {
            return Ok(None);
        };
        let command = std::str::from_utf8(&self.input[..end])
            .ok()
            .and_then(Command::parse);
        self.input.drain(..=end);
        command
            .map(Some)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid DBGp command"))
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if self.input.len() + bytes.len() > MAX_COMMAND_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DBGp command too long",
            ));
        }
        self.input.extend_from_slice(bytes);
        Ok(())
    }

    pub(super) async fn read(&mut self) -> io::Result<Command> {
        let mut buffer = [0; 4096];
        loop {
            if let Some(command) = self.next_command()? {
                return Ok(command);
            }
            let size = self.stream.read(&mut buffer).await?;
            self.append(&buffer[..size])?;
        }
    }

    pub(super) fn try_read(&mut self) -> io::Result<Option<Command>> {
        if let Some(command) = self.next_command()? {
            return Ok(Some(command));
        }
        let mut buffer = [0; 4096];
        match self.stream.try_read(&mut buffer) {
            Ok(size) => {
                self.append(&buffer[..size])?;
                self.next_command()
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn write(&mut self, xml: &str) -> io::Result<()> {
        const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
        let packet = format!("{}\0{DECLARATION}{xml}\0", DECLARATION.len() + xml.len());
        self.stream.write_all(packet.as_bytes()).await
    }
}

pub(super) fn file_uri(file: &str) -> String {
    url::Url::from_file_path(Path::new(file)).map_or_else(|()| file.to_owned(), |url| url.into())
}
