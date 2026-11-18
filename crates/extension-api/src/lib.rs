//! The contract between the runtime and native code.
//!
//! A native function sees PHP values (from `phera-value`) and a [`Context`],
//! never the VM's frames or bytecode, so the VM can change its internals
//! without breaking extensions.

mod arguments;
mod class;
mod context;
mod error;
mod function;
mod registry;
mod state;
pub mod testing;

pub use phera_value as value;
pub use phera_value::{ArrayKey, Diagnostics, Object, PhpArray, PhpString, Value};

pub use arguments::{args, prepare_arguments, prepare_call};
pub use class::{
    AsyncMethod, ClassEntry, ClassKind, MethodEntry, MethodPreparation, NativeMethod,
    PrepareMethod, PropertyEntry, SyncMethod, Visibility,
};
pub use context::{Context, LocalFuture};
pub use error::{NativeError, NativeResult, Unwind};
pub use function::{AsyncFn, FunctionEntry, NativeFn, Param, ParamType, SyncFn};
pub use registry::{ConstantValue, Registry};
pub use state::RequestState;

/// A compile-time linked bundle of PHP functions and constants.
pub trait Extension {
    fn name(&self) -> &'static str;

    fn register(&self, registry: &mut Registry);
}
