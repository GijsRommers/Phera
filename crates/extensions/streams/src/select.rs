//! `stream_select($read, $write, $except, 0)` retains only ready streams.

use std::collections::HashSet;
use std::os::fd::OwnedFd;
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
                let Ok((descriptor, buffered, is_child, wrapper)) =
                    with_stream(&resource, |stream| {
                        (
                            stream.descriptor().and_then(|file| file.try_clone().ok()),
                            stream.unread() > 0,
                            stream.child_pipe.is_some(),
                            stream.wrapper,
                        )
                    })
                else {
                    continue;
                };
                if let Some(fd) = descriptor {
                    child |= is_child;
                    candidates.push(Candidate {
                        direction,
                        key,
                        value: Value::Resource(resource),
                        fd: fd.into(),
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
                        "stream_select(): Cannot represent a stream of type {kind} as a select()able descriptor"
                    ).into());
                }
            }
        }
        if candidates.is_empty() {
            return Err(NativeError::value_error("No stream arrays were passed"));
        }
        let timeout = timeout(&arguments)?;
        let polling = wait(&candidates, timeout);
        let result = if let Some(budget) = child.then(|| context.process_wait_budget()).flatten() {
            tokio::time::timeout(budget, polling)
                .await
                .map_err(|_| context.process_wait_expired())?
        } else {
            polling.await
        };
        let Ok(ready) = result else {
            context.warning(
                format!(
                    "stream_select(): Unable to select [{}]",
                    result.unwrap_err()
                )
                .into(),
            );
            return Ok(Value::Bool(false));
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
    if micros.is_some_and(|micros| micros < 0) {
        return Err(NativeError::value_error(
            "stream_select(): Argument #5 ($microseconds) must be greater than or equal to 0",
        ));
    }
    if seconds.is_none() && micros.is_some() {
        return Err(NativeError::value_error(
            "stream_select(): Argument #5 ($microseconds) must be null when argument #4 ($seconds) is null",
        ));
    }
    Ok(seconds.map(|seconds| {
        Duration::from_secs(seconds as u64) + Duration::from_micros(micros.unwrap_or(0) as u64)
    }))
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
    loop {
        if poll(&mut fds, Some(&Timespec::default()))? != 0 {
            let ready = fds
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
                .collect();
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
