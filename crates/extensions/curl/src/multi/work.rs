//! `curl_multi_select($multi, 0.25)` waits for progress without blocking its worker.

use std::{collections::HashMap, future::poll_fn, rc::Rc, task::Poll, time::Duration};

use futures_util::FutureExt;
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

use super::{Handle, Multi, RECURSIVE, Work, handle, set_reference};
use crate::{
    constants::ids::*,
    transfer::{PoolOptions, Transfer},
};

pub(super) fn exec(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let multi = handle(&values)?;
        if multi.borrow().executing {
            multi.borrow_mut().error = RECURSIVE;
            return Ok(Value::Int(RECURSIVE));
        }
        multi.borrow_mut().executing = true;
        let guard = Executing(multi.clone());
        for _ in 0..32 {
            start(context, &multi).await?;
            if step(context, &multi).await? == 0 {
                let active = multi
                    .borrow()
                    .entries
                    .iter()
                    .any(|entry| matches!(entry.work, Work::Running(_)));
                if active {
                    // PHP may call exec repeatedly before select. Give Tokio's I/O driver a turn.
                    tokio::task::yield_now().await;
                }
                break;
            }
        }
        set_reference(
            &values,
            multi
                .borrow()
                .entries
                .iter()
                .filter(|entry| !matches!(entry.work, Work::Done))
                .count() as i64,
        )?;
        drop(guard);
        multi.borrow_mut().error = 0;
        Ok(Value::Int(0))
    })
}

struct Executing(Handle);

impl Drop for Executing {
    fn drop(&mut self) {
        let mut multi = self.0.borrow_mut();
        multi.executing = false;
        for entry in &mut multi.entries {
            if matches!(entry.work, Work::Stepping) {
                entry.work = Work::Done;
            }
        }
    }
}

fn host(entry: &super::Entry) -> Option<Rc<str>> {
    entry.handle.borrow().settings.origin()
}

async fn start(context: &mut dyn Context, multi: &Handle) -> Result<(), NativeError> {
    let (selected, pool) = {
        let multi = multi.borrow();
        let mut host_counts = HashMap::new();
        let mut total = 0;
        if multi.max_total != 0 || multi.max_host != 0 {
            for entry in &multi.entries {
                if matches!(entry.work, Work::Running(_)) {
                    total += 1;
                    if multi.max_host != 0 {
                        *host_counts.entry(host(entry)).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut selected = Vec::new();
        for (index, entry) in multi.entries.iter().enumerate() {
            if multi.max_total != 0 && total >= multi.max_total {
                break;
            }
            if !matches!(entry.work, Work::New) {
                continue;
            }
            if multi.max_host != 0 {
                let count = host_counts.entry(host(entry)).or_insert(0);
                if *count >= multi.max_host {
                    continue;
                }
                *count += 1;
            }
            total += 1;
            selected.push((index, entry.object.clone(), entry.handle.clone()));
        }
        (
            selected,
            PoolOptions {
                max_cached: multi.max_cached,
                multiplex: multi.multiplex,
            },
        )
    };
    for (index, object, handle) in selected {
        multi.borrow_mut().entries[index].work = Work::Stepping;
        let transfer = Transfer::start(context, object, handle, pool).await?;
        multi.borrow_mut().entries[index].work = Work::Running(Box::new(transfer));
    }
    Ok(())
}

async fn step(context: &mut dyn Context, multi: &Handle) -> Result<usize, NativeError> {
    let ready = poll_fn(|cx| {
        Poll::Ready(
            multi
                .borrow_mut()
                .entries
                .iter_mut()
                .enumerate()
                .filter_map(|(index, entry)| match &mut entry.work {
                    Work::Running(transfer) if !entry.handle.borrow().paused => {
                        transfer.poll_ready(cx).is_ready().then_some(index)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
        )
    })
    .await;
    let count = ready.len();
    for index in ready {
        let work = std::mem::replace(&mut multi.borrow_mut().entries[index].work, Work::Stepping);
        let Work::Running(mut transfer) = work else {
            continue;
        };
        if transfer.step(context).await?.is_ready() {
            let mut multi = multi.borrow_mut();
            let entry = &mut multi.entries[index];
            entry.work = Work::Done;
            let result = entry
                .handle
                .borrow()
                .error
                .as_ref()
                .map_or(CURLE_OK, |error| error.code);
            let object = entry.object.clone();
            multi.messages.push_back((object, result));
        } else {
            multi.borrow_mut().entries[index].work = Work::Running(transfer);
        }
    }
    Ok(count)
}

fn poll(multi: &mut Multi, cx: &mut std::task::Context<'_>) -> Poll<i64> {
    let mut running = false;
    let mut ready = 0;
    for entry in &mut multi.entries {
        if let Work::Running(transfer) = &mut entry.work {
            running = true;
            if !entry.handle.borrow().paused && transfer.poll_ready(cx).is_ready() {
                ready += 1;
            }
        }
    }
    if ready > 0 || !running {
        Poll::Ready(ready)
    } else {
        Poll::Pending
    }
}

pub(super) fn select(_: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let multi = handle(&values)?;
        if multi.borrow().executing {
            multi.borrow_mut().error = RECURSIVE;
            return Ok(Value::Int(-1));
        }
        let seconds = args::float(&values, 1).unwrap_or(1.0);
        if !seconds.is_finite() || seconds < 0.0 || seconds > f64::from(i32::MAX) / 1000.0 {
            return Err(NativeError::value_error(
                "curl_multi_select(): Argument #2 ($timeout) must be between 0 and 2147483.647000",
            ));
        }
        let wait = poll_fn(|cx| poll(&mut multi.borrow_mut(), cx));
        let ready = if seconds == 0.0 {
            wait.now_or_never().unwrap_or(0)
        } else {
            tokio::time::timeout(Duration::from_secs_f64(seconds), wait)
                .await
                .unwrap_or(0)
        };
        multi.borrow_mut().error = 0;
        Ok(Value::Int(ready))
    })
}
