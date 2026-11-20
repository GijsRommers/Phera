//! `fread($pipes[1], 8192)` waits for a child without blocking the PHP worker.

use std::fs::File;
use std::future::Future;
use std::io::{self, BufRead};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Value, value::Resource,
};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use tokio::io::Interest;
use tokio::net::UnixStream;
use tokio::net::unix::pipe::{Receiver, Sender};

use crate::request::new_resource;
use crate::resource::with_stream;
use crate::stream::{Backing, Stream, Wrapper};

#[derive(Clone)]
pub struct ChildPipe(Arc<Reactor>);

enum Reactor {
    Reader(Receiver),
    Writer(Sender),
    Socket(UnixStream),
    Terminal(Receiver, Sender),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChildStreamKind {
    Pipe,
    Socket,
    Terminal,
}

impl Reactor {
    async fn ready(&self, interest: Interest) -> io::Result<()> {
        match (self, interest) {
            (Self::Reader(pipe) | Self::Terminal(pipe, _), Interest::READABLE) => {
                pipe.readable().await
            }
            (Self::Writer(pipe) | Self::Terminal(_, pipe), Interest::WRITABLE) => {
                pipe.writable().await
            }
            (Self::Socket(socket), interest) => socket.ready(interest).await.map(|_| ()),
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    fn try_io<T>(
        &self,
        interest: Interest,
        operation: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        match self {
            Self::Reader(pipe) => pipe.try_io(operation),
            Self::Writer(pipe) => pipe.try_io(operation),
            Self::Socket(socket) => socket.try_io(interest, operation),
            Self::Terminal(reader, _) if interest == Interest::READABLE => reader.try_io(operation),
            Self::Terminal(_, writer) => writer.try_io(operation),
        }
    }
}

pub fn child_stream(
    context: &mut dyn Context,
    fd: OwnedFd,
    mode: &[u8],
    kind: ChildStreamKind,
) -> io::Result<Value> {
    fcntl_setfl(&fd, fcntl_getfl(&fd)? | OFlags::NONBLOCK)?;
    let copy = fd.as_fd().try_clone_to_owned()?;
    let reactor = match kind {
        ChildStreamKind::Socket => Reactor::Socket(UnixStream::from_std(
            std::os::unix::net::UnixStream::from(copy),
        )?),
        ChildStreamKind::Terminal => Reactor::Terminal(
            Receiver::from_owned_fd_unchecked(copy)?,
            Sender::from_owned_fd_unchecked(fd.as_fd().try_clone_to_owned()?)?,
        ),
        ChildStreamKind::Pipe if mode.contains(&b'r') => {
            Reactor::Reader(Receiver::from_owned_fd(copy)?)
        }
        ChildStreamKind::Pipe => Reactor::Writer(Sender::from_owned_fd(copy)?),
    };
    let wrapper = if kind == ChildStreamKind::Socket {
        Wrapper::ProcessSocket
    } else {
        Wrapper::Process
    };
    let mut stream = Stream::file(File::from(fd), wrapper, mode, b"");
    stream.child_pipe = Some(ChildPipe(Arc::new(reactor)));
    Ok(new_resource(context.state(), "stream", stream))
}

pub async fn open_descriptor(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    mode: &[u8],
) -> Result<Option<OwnedFd>, NativeError> {
    let stream = crate::wrapper::open(context, function, name, mode, false).await?;
    if stream.is_some() {
        crate::request::files(context).claim_id();
    }
    Ok(stream.and_then(|stream| stream.descriptor()?.as_fd().try_clone_to_owned().ok()))
}

pub fn duplicate_descriptor(resource: &Resource) -> Option<OwnedFd> {
    with_stream(resource, |stream| {
        stream.descriptor()?.as_fd().try_clone_to_owned().ok()
    })
    .ok()
    .flatten()
}

pub(crate) fn of(resource: &Resource) -> Result<Option<ChildPipe>, NativeError> {
    with_stream(resource, |stream| stream.child_pipe.clone())
}

pub(crate) async fn bounded<T>(
    context: &mut dyn Context,
    resource: &Resource,
    future: impl Future<Output = T>,
) -> Result<T, NativeError> {
    if of(resource)?.is_some() {
        wait_with_execution_limit(context, future).await
    } else {
        Ok(future.await)
    }
}

pub async fn wait_with_execution_limit<T>(
    context: &mut dyn Context,
    future: impl Future<Output = T>,
) -> Result<T, NativeError> {
    if let Some(duration) = context.process_wait_budget() {
        let start = tokio::time::Instant::now();
        let result = tokio::time::timeout(duration, future).await;
        context.process_wait_elapsed(start.elapsed());
        result.map_err(|_| context.process_wait_expired())
    } else {
        Ok(future.await)
    }
}

impl ChildPipe {
    pub(crate) async fn read(
        &self,
        resource: &Resource,
        length: Option<u64>,
    ) -> Result<io::Result<Vec<u8>>, NativeError> {
        self.collect(resource, length, false, length.is_some())
            .await
    }

    pub(crate) async fn line(
        &self,
        resource: &Resource,
        length: Option<u64>,
    ) -> Result<io::Result<Vec<u8>>, NativeError> {
        self.collect(resource, length, true, false).await
    }

    pub(crate) async fn contents(
        &self,
        resource: &Resource,
        length: Option<u64>,
    ) -> Result<io::Result<Vec<u8>>, NativeError> {
        self.collect(resource, length, false, false).await
    }

    async fn collect(
        &self,
        resource: &Resource,
        length: Option<u64>,
        line: bool,
        single: bool,
    ) -> Result<io::Result<Vec<u8>>, NativeError> {
        let blocked = with_stream(resource, |stream| stream.blocked)?;
        let limit = length.unwrap_or(u64::MAX);
        let mut output = Vec::new();
        while (output.len() as u64) < limit {
            let remaining = limit - output.len() as u64;
            let result = self
                .transfer(resource, blocked, Interest::READABLE, |stream| {
                    pull(stream, remaining, line, &mut output)
                })
                .await?;
            match result {
                Ok(true) => break,
                Ok(false) if single => break,
                Ok(false) => tokio::task::consume_budget().await,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error)
                    if matches!(&*self.0, Reactor::Terminal(..))
                        && error.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) =>
                {
                    with_stream(resource, |stream| stream.eof = true)?;
                    break;
                }
                Err(error) => return Ok(Err(error)),
            }
        }
        Ok(Ok(output))
    }

    pub(crate) async fn write(
        &self,
        resource: &Resource,
        bytes: &[u8],
    ) -> Result<io::Result<usize>, NativeError> {
        let blocked = with_stream(resource, |stream| stream.blocked)?;
        let mut written = 0;
        while written < bytes.len() {
            let result = self
                .transfer(resource, blocked, Interest::WRITABLE, |stream| {
                    stream.write(&bytes[written..])
                })
                .await?;
            match result {
                Ok(0) => break,
                Ok(count) => {
                    written += count;
                    tokio::task::consume_budget().await;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if written == 0 => return Ok(Err(error)),
                Err(_) => break,
            }
        }
        Ok(Ok(written))
    }

    async fn transfer<T>(
        &self,
        resource: &Resource,
        blocked: bool,
        interest: Interest,
        mut operation: impl FnMut(&mut Stream) -> io::Result<T>,
    ) -> Result<io::Result<T>, NativeError> {
        loop {
            match with_stream(resource, &mut operation)? {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock && blocked => break,
                result => return Ok(result),
            }
        }
        loop {
            if let Err(error) = self.0.ready(interest).await {
                return Ok(Err(error));
            }
            let result = with_stream(resource, |stream| {
                self.0.try_io(interest, || operation(stream))
            })?;
            match result {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                result => return Ok(result),
            }
        }
    }
}

fn pull(stream: &mut Stream, limit: u64, line: bool, output: &mut Vec<u8>) -> io::Result<bool> {
    let Backing::File(file) = &mut stream.backing else {
        return Err(io::ErrorKind::Unsupported.into());
    };
    let bytes = file.fill_buf()?;
    if bytes.is_empty() {
        stream.eof = true;
        return Ok(true);
    }
    let count = bytes
        .len()
        .min(usize::try_from(limit).unwrap_or(usize::MAX));
    let newline = line
        .then(|| bytes[..count].iter().position(|byte| *byte == b'\n'))
        .flatten();
    let count = newline.map_or(count, |index| index + 1);
    output
        .try_reserve(count)
        .map_err(|_| io::ErrorKind::OutOfMemory)?;
    output.extend_from_slice(&bytes[..count]);
    file.consume(count);
    stream.position += count as u64;
    Ok(newline.is_some())
}

/// A pclose()/fclose() hook attached to a process stream.
pub trait CloseAttachment: Send {
    fn close(self: Box<Self>, context: &mut dyn Context) -> LocalFuture<'_, NativeResult>;
}

pub fn set_attachment(
    resource: &Resource,
    attachment: Box<dyn CloseAttachment>,
) -> Result<(), NativeError> {
    with_stream(resource, |stream| stream.attachment = Some(attachment))
}

/// Close the stream before waiting, so a child reading stdin sees EOF.
pub async fn close_child_stream(context: &mut dyn Context, value: &Value) -> NativeResult {
    let resource = crate::resource::argument_resource(value, "pclose", "#1 ($handle)")?;
    if crate::directory::is_open(resource) {
        resource.data.borrow_mut().take();
        return Ok(Value::Int(0));
    }
    let resource = crate::resource::stream_argument(value, "pclose", "#1 ($handle)")?;
    let attachment = take_attachment(resource)?;
    crate::request::files(context)
        .stream_contexts
        .remove(&resource.id);
    match attachment {
        Some(attachment) => attachment.close(context).await,
        None => Ok(Value::Int(0)),
    }
}

pub(crate) fn take_attachment(
    resource: &Resource,
) -> Result<Option<Box<dyn CloseAttachment>>, NativeError> {
    let attachment = with_stream(resource, |stream| stream.attachment.take())?;
    resource.data.borrow_mut().take();
    Ok(attachment)
}
