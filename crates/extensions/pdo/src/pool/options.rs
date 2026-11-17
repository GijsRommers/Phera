//! Application policy is read once. A PHP constructor may only opt out of pooling.

use std::sync::OnceLock;
use std::time::Duration;

use phera_extension_api::value::to_bool;
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray};

use crate::errors::DbError;

pub const ENABLED: i64 = 100_000;

pub use phera_io_pool::Options;

fn environment() -> Result<Options, DbError> {
    let number = |name: &str, default: u64| -> Result<u64, DbError> {
        match std::env::var(name) {
            Err(std::env::VarError::NotPresent) => Ok(default),
            value => value
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0 && *value <= u32::MAX as u64)
                .ok_or_else(|| DbError {
                    state: "HY000".into(),
                    code: None,
                    detail: format!(
                        "{name} must be a positive integer no greater than {}",
                        u32::MAX
                    ),
                }),
        }
    };
    Ok(Options {
        size: number("PHERA_PDO_POOL_SIZE", 20)? as usize,
        idle: Duration::from_secs(number("PHERA_PDO_POOL_IDLE_SECONDS", 60)?),
        lifetime: Duration::from_secs(number("PHERA_PDO_POOL_LIFETIME_SECONDS", 1800)?),
        wait: Duration::from_secs(number("PHERA_PDO_POOL_WAIT_SECONDS", 30)?),
        io_threads: number("PHERA_PDO_IO_THREADS", 2)? as usize,
    })
}

pub fn application() -> Result<Options, DbError> {
    static OPTIONS: OnceLock<Result<Options, DbError>> = OnceLock::new();
    OPTIONS.get_or_init(environment).clone()
}

#[derive(Clone, Copy)]
pub enum Policy {
    Disabled,
    Enabled,
}

impl Policy {
    pub fn parse(_: &mut dyn Context, values: Option<&PhpArray>) -> Result<Self, NativeError> {
        if values
            .and_then(|values| values.get(&ArrayKey::Int(ENABLED)))
            .is_some_and(|value| !to_bool(value))
        {
            Ok(Self::Disabled)
        } else {
            Ok(Self::Enabled)
        }
    }
}

pub fn is_pool_attribute(attribute: i64) -> bool {
    attribute == ENABLED
}
