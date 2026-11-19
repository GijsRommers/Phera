//! `[1 => ['pipe', 'w'], 2 => ['redirect', 1]]` merges stderr into the stdout pipe.

use std::fs::OpenOptions;
use std::io::{self, stderr, stdin, stdout};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};

use command_fds::{CommandFdExt, FdMapping};
use phera_ext_streams::{ChildStreamKind, duplicate_descriptor, open_descriptor};
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

struct Descriptor {
    number: i32,
    child: OwnedFd,
    parent: Option<ParentPipe>,
}

pub(crate) struct ParentPipe {
    pub number: i32,
    pub fd: OwnedFd,
    pub mode: &'static [u8],
    pub kind: ChildStreamKind,
}

pub(crate) async fn prepare(
    context: &mut dyn Context,
    command: &mut Command,
    spec: &PhpArray,
) -> Result<Option<Vec<ParentPipe>>, NativeError> {
    let mut descriptors = Vec::with_capacity(spec.len());
    let mut pty = None;
    for (key, value) in spec.iter() {
        let ArrayKey::Int(number) = key else {
            return Err(spec_error("must be an integer indexed array"));
        };
        let number = number as i32;
        let descriptor = match value.deref().clone() {
            Value::Array(entry) => {
                from_array(context, &descriptors, &mut pty, number, &entry).await?
            }
            Value::Resource(resource) => match duplicate_descriptor(&resource) {
                Some(fd) => Some(simple(number, fd)),
                None => {
                    let reason = match phera_ext_streams::stream_kind(&resource) {
                        Some(kind) => {
                            format!("Cannot represent a stream of type {kind} as a File Descriptor")
                        }
                        None => {
                            return Err(NativeError::type_error(
                                "proc_open(): supplied resource is not a valid stream resource",
                            ));
                        }
                    };
                    context.warning(format!("proc_open(): {reason}"));
                    None
                }
            },
            _ => return Err(spec_error("must only contain arrays and streams")),
        };
        let Some(descriptor) = descriptor else {
            return Ok(None);
        };
        descriptors.push(descriptor);
    }
    let mut parents = Vec::new();
    let mut extras = Vec::new();
    for entry in descriptors {
        if let Some(parent) = entry.parent {
            parents.push(parent);
        }
        match entry.number {
            0 => {
                command.stdin(Stdio::from(entry.child));
            }
            1 => {
                command.stdout(Stdio::from(entry.child));
            }
            2 => {
                command.stderr(Stdio::from(entry.child));
            }
            child_fd => extras.push(FdMapping {
                parent_fd: entry.child,
                child_fd,
            }),
        }
    }
    if !extras.is_empty() {
        command
            .fd_mappings(extras)
            .map_err(|error| NativeError::value_error(format!("proc_open(): {error}")))?;
    }
    Ok(Some(parents))
}

async fn from_array(
    context: &mut dyn Context,
    descriptors: &[Descriptor],
    pty: &mut Option<(OwnedFd, OwnedFd)>,
    number: i32,
    entry: &PhpArray,
) -> Result<Option<Descriptor>, NativeError> {
    let kind = parameter(context, entry, 0, "handle qualifier").await?;
    let result = match kind.as_slice() {
        b"pipe" => {
            let mode = parameter(context, entry, 1, "mode parameter for 'pipe'").await?;
            make_pipe(number, mode.starts_with(b"w"))
        }
        b"file" => {
            let path = parameter(context, entry, 1, "file name parameter for 'file'").await?;
            let mode = parameter(context, entry, 2, "mode parameter for 'file'").await?;
            return Ok(open_descriptor(
                context,
                "proc_open",
                super::command::c_prefix(&path),
                super::command::c_prefix(&mode),
            )
            .await?
            .map(|fd| simple(number, fd)));
        }
        b"redirect" => {
            let target = entry
                .get(&ArrayKey::Int(1))
                .ok_or_else(|| NativeError::value_error("Missing redirection target"))?;
            let Value::Int(target) = target.deref().clone() else {
                return Err(NativeError::value_error(format!(
                    "Redirection target must be of type int, {} given",
                    given(target),
                )));
            };
            let target = target as i32;
            if !(0..=2).contains(&target) && !descriptors.iter().any(|d| d.number == target) {
                context.warning(format!(
                    "proc_open(): Redirection target {target} not found"
                ));
                return Ok(None);
            }
            redirect(descriptors, target).map(|fd| simple(number, fd))
        }
        b"null" => OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .map(|file| simple(number, file.into())),
        b"socket" => make_socket(number),
        b"pty" => make_pty(pty, number).await,
        _ => {
            let kind = String::from_utf8_lossy(super::command::c_prefix(&kind));
            context.warning(format!(
                "proc_open(): {kind} is not a valid descriptor spec/mode"
            ));
            return Ok(None);
        }
    };
    match result {
        Ok(descriptor) => Ok(Some(descriptor)),
        Err(error) => {
            let reason = super::command::reason(&error);
            let detail = match kind.as_slice() {
                b"pipe" => format!("Unable to create pipe {reason}"),
                b"socket" => format!("Unable to create socket pair: {reason}"),
                b"null" => format!("Failed to open /dev/null: {reason}"),
                b"pty" if error.kind() == io::ErrorKind::Unsupported => {
                    "PTY (pseudoterminal) not supported on this system".into()
                }
                b"pty" => format!("Could not open PTY (pseudoterminal): {reason}"),
                _ => format!("Failed to dup() for descriptor {number}: {reason}"),
            };
            context.warning(format!("proc_open(): {detail}"));
            Ok(None)
        }
    }
}

async fn parameter(
    context: &mut dyn Context,
    entry: &PhpArray,
    index: i64,
    name: &str,
) -> Result<Vec<u8>, NativeError> {
    let value = entry
        .get(&ArrayKey::Int(index))
        .ok_or_else(|| NativeError::value_error(format!("Missing {name}")))?;
    super::command::string(context, value).await
}

fn redirect(descriptors: &[Descriptor], target: i32) -> io::Result<OwnedFd> {
    if let Some(descriptor) = descriptors.iter().find(|entry| entry.number == target) {
        return descriptor.child.try_clone();
    }
    match target {
        0 => stdin().as_fd().try_clone_to_owned(),
        1 => stdout().as_fd().try_clone_to_owned(),
        2 => stderr().as_fd().try_clone_to_owned(),
        _ => Err(io::ErrorKind::InvalidInput.into()),
    }
}

fn simple(number: i32, child: OwnedFd) -> Descriptor {
    Descriptor {
        number,
        child,
        parent: None,
    }
}

fn make_pipe(number: i32, child_writes: bool) -> io::Result<Descriptor> {
    let (read, write) = io::pipe()?;
    let (read, write): (OwnedFd, OwnedFd) = (read.into(), write.into());
    let (parent, child, mode): (_, _, &[u8]) = if child_writes {
        (read, write, b"r")
    } else {
        (write, read, b"w")
    };
    Ok(Descriptor {
        number,
        child,
        parent: Some(ParentPipe {
            number,
            fd: parent,
            mode,
            kind: ChildStreamKind::Pipe,
        }),
    })
}

fn make_socket(number: i32) -> io::Result<Descriptor> {
    let (parent, child) = UnixStream::pair()?;
    Ok(Descriptor {
        number,
        child: child.into(),
        parent: Some(ParentPipe {
            number,
            fd: parent.into(),
            mode: b"r+",
            kind: ChildStreamKind::Socket,
        }),
    })
}

fn spec_error(message: &str) -> NativeError {
    NativeError::value_error(format!(
        "proc_open(): Argument #2 ($descriptor_spec) {message}"
    ))
}

pub(crate) fn given(value: &Value) -> String {
    match value.deref().clone() {
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        other => other.type_name(),
    }
}

async fn make_pty(pty: &mut Option<(OwnedFd, OwnedFd)>, number: i32) -> io::Result<Descriptor> {
    if pty.is_none() {
        *pty = Some(
            tokio::task::spawn_blocking(open_pty)
                .await
                .map_err(io::Error::other)??,
        );
    }
    let (master, slave) = pty.as_ref().ok_or(io::ErrorKind::Unsupported)?;
    Ok(Descriptor {
        number,
        child: slave.try_clone()?,
        parent: Some(ParentPipe {
            number,
            fd: master.try_clone()?,
            mode: b"r+",
            kind: ChildStreamKind::Terminal,
        }),
    })
}

fn open_pty() -> io::Result<(OwnedFd, OwnedFd)> {
    use rustix::fs::{Mode, OFlags, open};
    use rustix::pty::{grantpt, ptsname, unlockpt};

    let flags = OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC;
    let master = open("/dev/ptmx", flags, Mode::empty())?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let path = ptsname(&master, Vec::new())?;
    let slave = open(path.as_c_str(), flags, Mode::empty())?;
    Ok((master, slave))
}
