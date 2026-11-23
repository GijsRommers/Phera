//! The `$algo` and `$options` of `password_hash()` and
//! `password_needs_rehash()`. Bcrypt takes a cost, Argon2 a memory cost, a
//! time cost and a number of threads; each ignores the others' options:
//!
//! ```php
//! password_hash('secret', PASSWORD_BCRYPT, ['cost' => 10]);
//! password_hash('secret', PASSWORD_ARGON2ID, ['memory_cost' => 1 << 16, 'time_cost' => 4]);
//! password_hash('secret', PASSWORD_BCRYPT, ['cost' => 3]);  // ValueError
//! ```

use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value, args, value};

pub const DEFAULT_COST: i64 = 12;
/// In KiB.
pub const DEFAULT_MEMORY_COST: i64 = 65536;
pub const DEFAULT_TIME_COST: i64 = 4;
pub const DEFAULT_THREADS: i64 = 1;

/// Argon2 needs at least 8 KiB of memory per thread.
const MIN_MEMORY_PER_THREAD: i64 = 8;

/// libargon2's limit, which PHP checks before calling it.
const MAX_THREADS: i64 = 0xff_ffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Bcrypt,
    Argon2i,
    Argon2id,
}

impl Algorithm {
    /// The `$algo` argument: `null`, one of the `PASSWORD_*` strings, or
    /// the integers PHP used for them before 7.4.
    pub fn parse(value: &Value) -> Option<Self> {
        match value {
            Value::Null | Value::Int(0 | 1) => Some(Self::Bcrypt),
            Value::Int(2) => Some(Self::Argon2i),
            Value::Int(3) => Some(Self::Argon2id),
            Value::String(name) => match name.as_bytes() {
                b"2y" => Some(Self::Bcrypt),
                b"argon2i" => Some(Self::Argon2i),
                b"argon2id" => Some(Self::Argon2id),
                _ => None,
            },
            _ => None,
        }
    }

    /// The `PASSWORD_*` value, which `password_get_info()` calls `algo`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Bcrypt => "2y",
            Self::Argon2i => "argon2i",
            Self::Argon2id => "argon2id",
        }
    }

    /// What `password_get_info()` calls `algoName`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Bcrypt => "bcrypt",
            Self::Argon2i | Self::Argon2id => self.id(),
        }
    }
}

/// The costs of a password hash; bcrypt uses only `cost`, Argon2 the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub cost: i64,
    /// In KiB.
    pub memory: i64,
    pub time: i64,
    pub threads: i64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cost: DEFAULT_COST,
            memory: DEFAULT_MEMORY_COST,
            time: DEFAULT_TIME_COST,
            threads: DEFAULT_THREADS,
        }
    }
}

impl Options {
    /// The defaults, changed by what the `$options` argument (the third)
    /// sets for `algorithm`, each converted to int the way PHP does.
    pub fn read(
        context: &mut dyn Context,
        arguments: &[Value],
        algorithm: Algorithm,
    ) -> Result<Self, NativeError> {
        let mut result = Self::default();
        let Some(options) = args::array(arguments, 2) else {
            return Ok(result);
        };
        match algorithm {
            Algorithm::Bcrypt => read_int(context, options, "cost", &mut result.cost)?,
            Algorithm::Argon2i | Algorithm::Argon2id => {
                read_int(context, options, "memory_cost", &mut result.memory)?;
                read_int(context, options, "time_cost", &mut result.time)?;
                read_int(context, options, "threads", &mut result.threads)?;
            }
        }
        Ok(result)
    }

    /// PHP's checks of the costs, before anything is hashed.
    pub fn validate(&self, algorithm: Algorithm) -> Result<(), NativeError> {
        let error = match algorithm {
            Algorithm::Bcrypt if !(4..=31).contains(&self.cost) => {
                format!("Invalid bcrypt cost parameter specified: {}", self.cost)
            }
            Algorithm::Bcrypt => return Ok(()),
            _ if !(MIN_MEMORY_PER_THREAD..=i64::from(u32::MAX)).contains(&self.memory) => {
                "Memory cost is outside of allowed memory range".to_string()
            }
            _ if !(1..=i64::from(u32::MAX)).contains(&self.time) => {
                "Time cost is outside of allowed time range".to_string()
            }
            _ if !(1..=MAX_THREADS).contains(&self.threads) => {
                "Invalid number of threads".to_string()
            }
            _ if self.memory < MIN_MEMORY_PER_THREAD * self.threads => {
                "Memory cost is too small".to_string()
            }
            _ => return Ok(()),
        };
        Err(NativeError::value_error(error))
    }
}

/// Sets `slot` from `options[name]`, if it is there.
fn read_int(
    context: &mut dyn Context,
    options: &PhpArray,
    name: &str,
    slot: &mut i64,
) -> Result<(), NativeError> {
    if let Some(option) = options.get(&ArrayKey::str(name)) {
        *slot = value::to_int(option, context.diagnostics())?;
    }
    Ok(())
}
