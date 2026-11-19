//! `posix_times()['utime']` reports user CPU time in system clock ticks.

use nix::sys::resource::{UsageWho, getrusage};
use nix::sys::time::TimeValLike;
use nix::unistd::{SysconfVar, sysconf};
use phera_extension_api::{Context, FunctionEntry, NativeResult, Registry, Value};
use rustix::time::{ClockId, clock_gettime};

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("posix_times", &[], times));
    registry.function(FunctionEntry::sync("posix_uname", &[], uname));
}

fn uname(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let name = rustix::system::uname();
    let fields = [
        ("sysname", Value::from(name.sysname().to_bytes())),
        ("nodename", Value::from(name.nodename().to_bytes())),
        ("release", Value::from(name.release().to_bytes())),
        ("version", Value::from(name.version().to_bytes())),
        ("machine", Value::from(name.machine().to_bytes())),
        #[cfg(target_os = "linux")]
        ("domainname", Value::from(name.domainname().to_bytes())),
    ];
    Ok(super::account::array(fields))
}

fn times(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let frequency = sysconf(SysconfVar::CLK_TCK).ok().flatten().unwrap_or(100);
    let process = match getrusage(UsageWho::RUSAGE_SELF) {
        Ok(usage) => usage,
        Err(error) => return Ok(super::fail(context, error)),
    };
    let children = match getrusage(UsageWho::RUSAGE_CHILDREN) {
        Ok(usage) => usage,
        Err(error) => return Ok(super::fail(context, error)),
    };
    let clock = if cfg!(target_vendor = "apple") {
        ClockId::Realtime
    } else {
        ClockId::Monotonic
    };
    let now = clock_gettime(clock);
    let ticks = now.tv_sec.saturating_mul(frequency)
        + now.tv_nsec.saturating_mul(frequency) / 1_000_000_000;
    let to_ticks = |micros: i64| Value::Int(micros.saturating_mul(frequency) / 1_000_000);
    Ok(super::account::array([
        ("ticks", Value::Int(ticks)),
        ("utime", to_ticks(process.user_time().num_microseconds())),
        ("stime", to_ticks(process.system_time().num_microseconds())),
        ("cutime", to_ticks(children.user_time().num_microseconds())),
        (
            "cstime",
            to_ticks(children.system_time().num_microseconds()),
        ),
    ]))
}
