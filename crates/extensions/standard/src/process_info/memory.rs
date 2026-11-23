//! `memory_get_usage(true)` reports process resident bytes. The request has
//! no separate PHP allocator, so both values of `real_usage` use RSS.
//! `memory_reset_peak_usage()` starts a fresh high-water mark sampled by
//! subsequent memory calls in that request.

use nix::sys::resource::{UsageWho, getrusage};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry,
    Value,
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, get_current_pid};

#[derive(Default)]
pub(crate) struct Peak {
    since_reset: Option<i64>,
}

pub(super) fn register(registry: &mut Registry) {
    const REAL: &[Param] =
        &[Param::new("real_usage", ParamType::Bool).with_default(ConstantValue::Bool(false))];
    for entry in [
        FunctionEntry::asynchronous("memory_get_usage", REAL, usage),
        FunctionEntry::asynchronous("memory_get_peak_usage", REAL, peak),
        FunctionEntry::asynchronous("memory_reset_peak_usage", &[], reset),
    ] {
        registry.function(entry);
    }
}

async fn resident() -> i64 {
    tokio::task::spawn_blocking(|| {
        let pid = get_current_pid().ok()?;
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let bytes = system.process(pid)?.memory();
        i64::try_from(bytes).ok()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_else(process_peak)
}

fn process_peak() -> i64 {
    getrusage(UsageWho::RUSAGE_SELF).map_or(0, |usage| {
        if cfg!(target_vendor = "apple") {
            usage.max_rss()
        } else {
            usage.max_rss().saturating_mul(1024)
        }
    })
}

fn usage(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let size = resident().await;
        if let Some(peak) = &mut context.state().get_or_default::<Peak>().since_reset {
            *peak = (*peak).max(size);
        }
        Ok(Value::Int(size))
    })
}

fn peak(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let size = resident().await;
        let peak = context.state().get_or_default::<Peak>();
        let value = match &mut peak.since_reset {
            Some(maximum) => {
                *maximum = (*maximum).max(size);
                *maximum
            }
            None => process_peak().max(size),
        };
        Ok(Value::Int(value))
    })
}

fn reset(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let size = resident().await;
        context.state().get_or_default::<Peak>().since_reset = Some(size);
        Ok(Value::Null)
    })
}
