//! Requests from `ReflectionClass::newLazyGhost($initializer)` and related methods.

use std::rc::Rc;

pub use phera_value::LazyStrategy;
use phera_value::{Object, Value};

pub const SKIP_INITIALIZATION_ON_SERIALIZE: i64 = 8;
pub const SKIP_DESTRUCTOR: i64 = 16;

pub struct MakeLazy {
    pub object: Option<Rc<Object>>,
    pub initializer: Value,
    pub strategy: LazyStrategy,
    pub method: &'static str,
    pub options: i64,
}

#[derive(Clone, Copy)]
pub enum LazyClassQuery {
    Initialize,
    MarkInitialized,
    IsUninitialized,
    GetInitializer,
}

pub enum LazyPropertyOperation {
    IsLazy,
    Skip,
    Write(Value),
}

impl LazyPropertyOperation {
    pub fn method(&self) -> &'static str {
        match self {
            Self::IsLazy => "isLazy",
            Self::Skip => "skipLazyInitialization",
            Self::Write(_) => "setRawValueWithoutLazyInitialization",
        }
    }
}
