//! `get_headers($url, true)` retains the spelling and order sent by the server.

use std::{
    io,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
};

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, PhpArray, PhpString, Registry, Value,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[derive(Default)]
struct LastHeaders(Option<Rc<PhpArray>>);

pub(super) fn register(registry: &mut Registry) {
    registry.fork_state::<LastHeaders>(phera_extension_api::fresh_state);
    registry.function(FunctionEntry::sync(
        "http_get_last_response_headers",
        &[],
        get_last,
    ));
    registry.function(FunctionEntry::sync(
        "http_clear_last_response_headers",
        &[],
        clear_last,
    ));
}

fn get_last(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context
        .state()
        .get::<LastHeaders>()
        .and_then(|state| state.0.clone())
        .map_or(Value::Null, Value::Array))
}

fn clear_last(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    clear(context);
    Ok(Value::Null)
}

pub(super) fn clear(context: &mut dyn Context) {
    context.state().take::<LastHeaders>();
}

pub(super) fn save(
    context: &mut dyn Context,
    lines: &[Vec<u8>],
) -> Result<(), phera_extension_api::NativeError> {
    let headers = Rc::new(values(lines, false)?);
    context.state().get_or_default::<LastHeaders>().0 = Some(headers.clone());
    context.replace_caller_variable("http_response_header", Value::Array(headers))
}

pub(super) fn values(
    lines: &[Vec<u8>],
    associative: bool,
) -> Result<PhpArray, phera_extension_api::NativeError> {
    let mut result = PhpArray::new();
    for line in lines {
        if let Some((name, value)) =
            field(line).filter(|_| associative && !line.starts_with(b"HTTP/"))
        {
            let key = ArrayKey::from_string(PhpString::from(name));
            let value = Value::from(value.to_vec());
            match result.get_mut(&key) {
                Some(Value::Array(list)) => {
                    Rc::make_mut(list).push(value)?;
                }
                Some(first) => {
                    *first = Value::from(PhpArray::from_values([first.clone(), value], 2));
                }
                None => {
                    result.insert(key, value);
                }
            }
        } else {
            result.push(Value::from(line.clone()))?;
        }
    }
    Ok(result)
}

pub(super) fn field(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let position = line.iter().position(|&byte| byte == b':')?;
    Some((&line[..position], line[position + 1..].trim_ascii()))
}

// Hyper owns framing, validation and body decoding. It exposes a normalized
// HeaderMap, so retain the bounded wire header alongside the parsed response.
pub(super) struct ReadHeaders<T> {
    channel: T,
    capture: Capture,
    complete: bool,
    block_start: usize,
}

#[derive(Clone, Default)]
pub(super) struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    pub(super) fn lines(&self) -> Vec<Vec<u8>> {
        let bytes = self.0.lock().expect("HTTP header capture");
        let mut lines: Vec<Vec<u8>> = Vec::new();
        for line in bytes.split(|&byte| byte == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() {
                if is_informational(&lines) {
                    lines.clear();
                    continue;
                }
                break;
            }
            if line.first().is_some_and(u8::is_ascii_whitespace)
                && let Some(previous) = lines.last_mut()
            {
                previous.push(b' ');
                previous.extend_from_slice(line.trim_ascii());
            } else {
                lines.push(line.trim_ascii_end().to_vec());
            }
        }
        lines
    }
}

impl<T> ReadHeaders<T> {
    pub(super) fn new(channel: T, capture: Capture) -> Self {
        Self {
            channel,
            capture,
            complete: false,
            block_start: 0,
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for ReadHeaders<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let start = buffer.filled().len();
        let result = Pin::new(&mut self.channel).poll_read(context, buffer);
        if !self.complete && matches!(result, Poll::Ready(Ok(()))) {
            let this = &mut *self;
            this.complete = capture(
                &this.capture,
                &mut this.block_start,
                &buffer.filled()[start..],
            );
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for ReadHeaders<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.channel).poll_write(context, data)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.channel).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.channel).poll_shutdown(context)
    }
}

fn capture(capture: &Capture, block_start: &mut usize, data: &[u8]) -> bool {
    const LIMIT: usize = 128 * 1024;
    let mut bytes = capture.0.lock().expect("HTTP header capture");
    let mut scan = bytes.len().saturating_sub(3).max(*block_start);
    let count = LIMIT.saturating_sub(bytes.len()).min(data.len());
    bytes.extend_from_slice(&data[..count]);
    while let Some(end) = header_end(&bytes[scan..]).map(|end| scan + end) {
        if !informational(&bytes[*block_start..end]) {
            bytes.truncate(end);
            return true;
        }
        *block_start = end;
        scan = end;
    }
    bytes.len() == LIMIT
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(2)
        .position(|line| line == b"\n\n")
        .map(|offset| offset + 2)
        .into_iter()
        .chain(
            bytes
                .windows(4)
                .position(|line| line == b"\r\n\r\n")
                .map(|offset| offset + 4),
        )
        .min()
}

fn is_informational(lines: &[Vec<u8>]) -> bool {
    lines.first().is_some_and(|line| informational(line))
}

fn informational(header: &[u8]) -> bool {
    let status = header.split(|byte| byte.is_ascii_whitespace()).nth(1);
    matches!(status, Some([b'1', b'0'..=b'9', b'0'..=b'9'])) && status != Some(b"101")
}
