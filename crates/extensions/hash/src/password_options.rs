//! `password_hash($password, PASSWORD_BCRYPT, ['cost'=>12])` validates options first.
use phera_extension_api::{ArrayKey, Context, NativeError, Value, args, value};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Bcrypt,
    Argon2i,
    Argon2id,
}
impl Algorithm {
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        match value {
            None | Some(Value::Null) | Some(Value::Int(0 | 1)) => Some(Self::Bcrypt),
            Some(Value::Int(2)) => Some(Self::Argon2i),
            Some(Value::Int(3)) => Some(Self::Argon2id),
            Some(Value::String(s)) => match s.as_bytes() {
                b"2y" => Some(Self::Bcrypt),
                b"argon2i" => Some(Self::Argon2i),
                b"argon2id" => Some(Self::Argon2id),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Bcrypt => "2y",
            Self::Argon2i => "argon2i",
            Self::Argon2id => "argon2id",
        }
    }
    pub fn name(self) -> &'static str {
        if self == Self::Bcrypt {
            "bcrypt"
        } else {
            self.id()
        }
    }
}
#[derive(Clone)]
pub struct Options {
    pub cost: i64,
    pub memory: i64,
    pub time: i64,
    pub threads: i64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            cost: 12,
            memory: 65536,
            time: 4,
            threads: 1,
        }
    }
}
impl Options {
    pub fn read(context: &mut dyn Context, arguments: &[Value]) -> Result<Self, NativeError> {
        let mut result = Self::default();
        if let Some(options) = args::array(arguments, 2) {
            for (name, slot) in [
                ("cost", &mut result.cost),
                ("memory_cost", &mut result.memory),
                ("time_cost", &mut result.time),
                ("threads", &mut result.threads),
            ] {
                if let Some(value) = options.get(&ArrayKey::str(name)) {
                    *slot = value::to_int(value, context.diagnostics())?;
                }
            }
        }
        Ok(result)
    }
    pub fn validate(&self, algorithm: Algorithm, password: &[u8]) -> Result<(), NativeError> {
        let error = if algorithm == Algorithm::Bcrypt {
            if password.contains(&0) {
                Some("Bcrypt password must not contain null character".into())
            } else if !(4..=31).contains(&self.cost) {
                Some(format!(
                    "Invalid bcrypt cost parameter specified: {}",
                    self.cost
                ))
            } else {
                None
            }
        } else if !(8..=u32::MAX as i64).contains(&self.memory) {
            Some("Memory cost is outside of allowed memory range".into())
        } else if !(1..=u32::MAX as i64).contains(&self.time) {
            Some("Time cost is outside of allowed time range".into())
        } else if !(1..=0xffffff).contains(&self.threads) {
            Some("Invalid number of threads".into())
        } else if self.memory < 8 * self.threads {
            Some("Memory cost is too small".into())
        } else {
            None
        };
        if let Some(error) = error {
            Err(NativeError::value_error(error))
        } else {
            Ok(())
        }
    }
}
