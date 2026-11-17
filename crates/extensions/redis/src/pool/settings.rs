//! Application limits are environment settings, never PHP request options.
use phera_io_pool::Options;
use redis::RedisResult;
use std::sync::OnceLock;
use std::time::Duration;

pub fn application() -> RedisResult<Options> {
    static OPTIONS: OnceLock<Result<Options, String>> = OnceLock::new();
    OPTIONS
        .get_or_init(|| {
            let number = |suffix: &str, default: usize| -> Result<usize, String> {
                let name = format!("PHERA_REDIS_{suffix}");
                match std::env::var(&name) {
                    Err(std::env::VarError::NotPresent) => Ok(default),
                    value => value
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .filter(|n| *n > 0 && *n <= u32::MAX as usize)
                        .ok_or_else(|| format!("{name} must be a positive integer")),
                }
            };
            Ok(Options {
                size: number("POOL_SIZE", 512)?,
                idle: Duration::from_secs(number("POOL_IDLE_SECONDS", 60)? as u64),
                lifetime: Duration::from_secs(number("POOL_LIFETIME_SECONDS", 1800)? as u64),
                wait: Duration::from_secs(number("POOL_WAIT_SECONDS", 30)? as u64),
                io_threads: number("IO_THREADS", 2)?,
            })
        })
        .clone()
        .map_err(super::error)
}
