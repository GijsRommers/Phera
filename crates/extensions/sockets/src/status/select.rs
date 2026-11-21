//! `socket_select($read, $write, $except, 0)` keeps only sockets that are ready.

use std::{
    collections::HashSet,
    os::fd::{AsFd, AsRawFd},
    sync::Arc,
    time::Duration,
};

use nix::libc;
use phera_ext_streams::SocketConnection;
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};
use rustix::event::{PollFd, PollFlags, poll};

use crate::{classes, failure};

const FIELDS: [&str; 3] = ["read", "write", "except"];
const READINESS: [PollFlags; 3] = [PollFlags::IN, PollFlags::OUT, PollFlags::PRI];

pub(super) fn register(registry: &mut Registry) {
    const ARRAYS: &[Param] = &[
        Param::new("read", ParamType::Array).nullable().by_ref(),
        Param::new("write", ParamType::Array).nullable().by_ref(),
        Param::new("except", ParamType::Array).nullable().by_ref(),
        Param::new("seconds", ParamType::Int).nullable(),
        Param::new("microseconds", ParamType::Int).with_default(ConstantValue::Int(0)),
    ];
    registry.function(FunctionEntry::asynchronous("socket_select", ARRAYS, select));
}

struct Candidate {
    group: usize,
    key: ArrayKey,
    value: Value,
    socket: Arc<SocketConnection>,
}

fn select(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const FUNCTION: &str = "socket_select";
        let mut candidates = Vec::new();
        for (group, field) in FIELDS.into_iter().enumerate() {
            let value = args::value(&args, group).deref();
            let Value::Array(array) = &*value else {
                continue;
            };
            for (key, object) in array.iter() {
                let resolved = object.deref();
                if !matches!(&*resolved, Value::Object(object) if object.class.name() == "Socket") {
                    return Err(NativeError::type_error(format!(
                        "{FUNCTION}(): Argument #{} (${field}) must only have elements of type Socket, {} given",
                        group + 1,
                        resolved.type_name()
                    )));
                }
                let handle = classes::argument(std::slice::from_ref(object), FUNCTION)?;
                match handle.raw(FUNCTION)?.connect().await {
                    Ok(socket) => candidates.push(Candidate {
                        group,
                        key: key.clone(),
                        value: object.clone(),
                        socket,
                    }),
                    Err(error) => {
                        return failure(context, None, FUNCTION, "Unable to select", &error);
                    }
                };
            }
        }
        if candidates.is_empty() {
            return Err(NativeError::value_error(format!(
                "{FUNCTION}(): At least one array argument must be passed"
            )));
        }
        let seconds = args::int(&args, 3);
        let micros = args::int(&args, 4).unwrap_or_default();
        if seconds.is_some_and(|seconds| seconds < 0 || micros < 0) {
            return failure(
                context,
                None,
                FUNCTION,
                "Unable to select",
                &std::io::Error::from_raw_os_error(libc::EINVAL),
            );
        }
        let timeout = seconds.map(|seconds| {
            Duration::from_secs(seconds as u64).saturating_add(Duration::from_micros(micros as u64))
        });
        match wait(&candidates, timeout).await {
            Ok(ready) => apply(&args, &candidates, ready),
            Err(error) => failure(context, None, FUNCTION, "Unable to select", &error.into()),
        }
    })
}

async fn wait(
    candidates: &[Candidate],
    timeout: Option<Duration>,
) -> rustix::io::Result<Vec<usize>> {
    let start = tokio::time::Instant::now();
    let mut fds: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            PollFd::from_borrowed_fd(candidate.socket.as_fd(), READINESS[candidate.group])
        })
        .collect();
    loop {
        let count = poll(
            &mut fds,
            Some(&rustix::event::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )?;
        if count != 0 {
            let ready = fds
                .iter()
                .zip(candidates)
                .enumerate()
                .filter_map(|(index, (fd, candidate))| {
                    let mut want = READINESS[candidate.group];
                    if candidate.group == 0 {
                        want |= PollFlags::HUP | PollFlags::ERR;
                    }
                    fd.revents().intersects(want).then_some(index)
                })
                .collect::<Vec<_>>();
            if !ready.is_empty() {
                return Ok(ready);
            }
        }
        let interval = Duration::from_millis(1);
        let delay = timeout.map_or(interval, |limit| {
            interval.min(limit.saturating_sub(start.elapsed()))
        });
        if delay.is_zero() {
            return Ok(Vec::new());
        }
        tokio::time::sleep(delay).await;
    }
}

fn apply(args: &[Value], candidates: &[Candidate], ready: Vec<usize>) -> NativeResult {
    let mut updated = [PhpArray::new(), PhpArray::new(), PhpArray::new()];
    let mut unique = HashSet::new();
    for index in ready {
        let candidate = &candidates[index];
        updated[candidate.group].insert(candidate.key.clone(), candidate.value.clone());
        unique.insert((candidate.group, candidate.socket.as_fd().as_raw_fd()));
    }
    for (index, array) in updated.into_iter().enumerate() {
        if !args::value(args, index).is_null()
            && let Some(target) = args::reference(args, index)
        {
            *target.borrow_mut() = Value::from(array);
        }
    }
    Ok(Value::Int(unique.len() as i64))
}
