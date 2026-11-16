//! The contract between the runtime and native code.
//!
//! A native function sees PHP values (from `phera-value`) and a [`Context`],
//! never the VM's frames or bytecode, so the VM can change its internals
//! without breaking extensions. Native classes are not expressible yet.

mod arguments;
mod context;
mod error;
mod function;
mod registry;
pub mod testing;

pub use phera_value as value;
pub use phera_value::{ArrayKey, Diagnostics, PhpArray, PhpString, Value};

pub use arguments::{args, prepare_arguments};
pub use context::{Context, LocalFuture};
pub use error::{NativeError, NativeResult, Unwind};
pub use function::{AsyncFn, FunctionEntry, NativeFn, Param, ParamType, SyncFn};
pub use registry::{ConstantValue, Registry};

/// A compile-time linked bundle of PHP functions and constants.
pub trait Extension {
    fn name(&self) -> &'static str;

    fn register(&self, registry: &mut Registry);
}
