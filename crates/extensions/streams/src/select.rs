//! `stream_select($read, $write, $except, 0)` retains only ready streams.

use nix::sys::{
    select::{FD_SETSIZE, FdSet, select as select_fds},
    time::TimeVal,
};
use std::collections::HashSet;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::time::Duration;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};

use crate::resource::with_stream;
use crate::stream::Wrapper;

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("read", ParamType::Array).nullable().by_ref(),
        Param::new("write", ParamType::Array).nullable().by_ref(),
        Param::new("except", ParamType::Array).nullable().by_ref(),
        Param::new("seconds", ParamType::Int).nullable(),
        Param::new("microseconds", ParamType::Int)
            .nullable()
            .optional(),
    ];
    registry.function(FunctionEntry::asynchronous("stream_select", PARAMS, select));
}

struct Candidate {
    direction: usize,
    key: ArrayKey,
    value: Value,
    fd: OwnedFd,
    original_fd: i32,
    buffered: bool,
}

fn select(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut candidates = Vec::new();
        let mut child = false;
        for direction in 0..3 {
            let Value::Array(array) = args::value(&arguments, direction).deref().clone() else {
                continue;
            };
            for (key, value) in array.iter() {
                let Value::Resource(resource) = value.deref().clone() else {
                    continue;
                };
                let user = crate::user_file::resource_file(&resource);
                let descriptor_resource = if let Some(file) = &user {
                    let Some(real) = file.cast_for_select(context, resource.id).await? else {
                        continue;
                    };
                    real
                } else {
                    resource.clone()
                };
                let Ok((descriptor, buffered, is_child, wrapper)) =
                    with_stream(&descriptor_resource, |stream| {
                        (
                            stream.descriptor().and_then(|file| {
                                file.try_clone().ok().map(|copy| (file.as_raw_fd(), copy))
                            }),
                            user.as_ref().map_or_else(
                                || stream.unread() > 0,
                                |file| !file.buffer.borrow().is_empty(),
                            ),
                            stream.child_pipe.is_some(),
                            stream.wrapper,
                        )
                    })
                else {
                    continue;
                };
                if let Some((original_fd, fd)) = descriptor {
                    child |= is_child;
                    candidates.push(Candidate {
                        direction,
                        key,
                        value: Value::Resource(resource),
                        fd: fd.into(),
                        original_fd,
                        buffered,
                    });
                } else {
                    let kind = match wrapper {
                        Wrapper::Memory => "MEMORY",
                        Wrapper::Temp => "TEMP",
                        Wrapper::Data => "RFC2397",
                        Wrapper::Input => "Input",
                        _ => "Output",
                    };
                    context.warning(format!(
                        "stream_select(): Cannot represent a stream of type {kind} \
                         as a select()able descriptor"
                    ));
                }
            }
        }
        if candidates.is_empty() {
            let error = NativeError::value_error("No stream arrays were passed");
            return Err(context.native_call_error("stream_select", &arguments, error));
        }
        let timeout = timeout(&arguments)
            .map_err(|error| context.native_call_error("stream_select", &arguments, error))?;
        let max_fd = candidates
            .iter()
            .map(|candidate| candidate.original_fd)
            .max()
            .unwrap_or(0);
        if max_fd >= FD_SETSIZE as i32 {
            context.warning(format!(
                "stream_select(): select() cannot monitor descriptor {max_fd} \
                 (FD_SETSIZE={FD_SETSIZE})"
            ));
            return Ok(Value::Bool(false));
        }
        if timeout.is_some_and(|duration| !valid_timeout(duration)) {
            return Ok(select_error(context, rustix::io::Errno::INVAL, max_fd));
        }
        let polling = wait(&candidates, timeout);
        let result = if child {
            crate::child_pipe::wait_with_execution_limit(context, polling).await?
        } else {
            polling.await
        };
        let ready = match result {
            Ok(ready) => ready,
            Err(error) => return Ok(select_error(context, error, max_fd)),
        };
        let mut arrays = [PhpArray::new(), PhpArray::new(), PhpArray::new()];
        let mut descriptors = HashSet::new();
        for index in ready {
            let candidate = &candidates[index];
            if let Value::Resource(resource) = &candidate.value {
                descriptors.insert((candidate.direction, resource.id));
            }
            arrays[candidate.direction].insert(candidate.key.clone(), candidate.value.clone());
        }
        for (arg, filtered) in arguments.iter().zip(arrays) {
            if let Value::Reference(reference) = arg
                && !reference.borrow().is_null()
            {
                *reference.borrow_mut() = Value::from(filtered);
            }
        }
        Ok(Value::Int(descriptors.len() as i64))
    })
}

fn timeout(arguments: &[Value]) -> Result<Option<Duration>, NativeError> {
    let seconds = args::int(arguments, 3);
    let micros = args::int(arguments, 4);
    if seconds.is_some_and(|seconds| seconds < 0) {
        return Err(NativeError::value_error(
            "stream_select(): Argument #4 ($seconds) must be greater than or equal to 0",
        ));
    }
    if seconds.is_some() && micros.is_some_and(|micros| micros < 0) {
        return Err(NativeError::value_error(
            "stream_select(): Argument #5 ($microseconds) must be greater than or equal to 0",
        ));
    }
    if seconds.is_none() && micros.is_some_and(|micros| micros != 0) {
        return Err(NativeError::value_error(
            "stream_select(): Argument #5 ($microseconds) must be null \
             when argument #4 ($seconds) is null",
        ));
    }
    Ok(seconds.map(|seconds| {
        Duration::from_secs(seconds as u64) + Duration::from_micros(micros.unwrap_or(0) as u64)
    }))
}

fn valid_timeout(duration: Duration) -> bool {
    let max = if cfg!(target_vendor = "apple") {
        i32::MAX as u64 - 1
    } else {
        i64::MAX as u64
    };
    duration.as_secs() <= max
}

fn select_error(context: &mut dyn Context, error: rustix::io::Errno, max_fd: i32) -> Value {
    let number = error.raw_os_error();
    let description = nix::errno::Errno::from_raw(number).desc();
    context.warning(format!(
        "stream_select(): Unable to select [{number}]: {description} (max_fd={max_fd})"
    ));
    Value::Bool(false)
}

async fn wait(
    candidates: &[Candidate],
    timeout: Option<Duration>,
) -> Result<Vec<usize>, rustix::io::Errno> {
    let buffered: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            (candidate.direction == 0 && candidate.buffered).then_some(index)
        })
        .collect();
    if !buffered.is_empty() {
        return Ok(buffered);
    }
    let start = tokio::time::Instant::now();
    loop {
        let ready = readiness(candidates)?;
        if !ready.is_empty() {
            return Ok(ready);
        }
        let interval = Duration::from_millis(1);
        let pause = timeout.map_or(interval, |duration| {
            interval.min(duration.saturating_sub(start.elapsed()))
        });
        if pause.is_zero() {
            return Ok(Vec::new());
        }
        tokio::time::sleep(pause).await;
    }
}

fn readiness(candidates: &[Candidate]) -> Result<Vec<usize>, rustix::io::Errno> {
    if candidates
        .iter()
        .any(|candidate| candidate.fd.as_raw_fd() >= FD_SETSIZE as i32)
    {
        return poll_readiness(candidates);
    }
    let mut sets = [FdSet::new(), FdSet::new(), FdSet::new()];
    for candidate in candidates {
        sets[candidate.direction].insert(candidate.fd.as_fd());
    }
    let [read, write, except] = &mut sets;
    select_fds(None, read, write, except, &mut TimeVal::new(0, 0))
        .map_err(|error| rustix::io::Errno::from_raw_os_error(error as i32))?;
    Ok(candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            sets[candidate.direction]
                .contains(candidate.fd.as_fd())
                .then_some(index)
        })
        .collect())
}

/// Duplicating an otherwise selectable PHP descriptor can exceed FD_SETSIZE.
fn poll_readiness(candidates: &[Candidate]) -> Result<Vec<usize>, rustix::io::Errno> {
    let mut fds = candidates
        .iter()
        .map(|candidate| {
            PollFd::new(
                &candidate.fd,
                match candidate.direction {
                    0 => PollFlags::IN,
                    1 => PollFlags::OUT,
                    _ => PollFlags::PRI,
                },
            )
        })
        .collect::<Vec<_>>();
    poll(&mut fds, Some(&Timespec::default()))?;
    Ok(fds
        .iter()
        .enumerate()
        .filter_map(|(index, fd)| {
            let events = match candidates[index].direction {
                0 => PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
                1 => PollFlags::OUT | PollFlags::ERR,
                _ => PollFlags::PRI,
            };
            fd.revents().intersects(events).then_some(index)
        })
        .collect())
}
