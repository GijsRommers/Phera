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
mod object_handlers;
mod registry;
mod state;
pub mod testing;
pub mod traversal;

pub use phera_source::{CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite};
pub use phera_value as value;
pub use phera_value::{ArrayKey, Diagnostics, Object, PhpArray, PhpString, Value};

pub use arguments::{args, prepare_arguments, prepare_call};
pub use class::{
    AsyncMethod, ClassEntry, ClassKind, MethodEntry, MethodPreparation, MethodReturn, NativeMethod,
    NativePropertyAccess, NativePropertyMutation, NativePropertyRead, PrepareMethod, PropertyEntry,
    PropertyType, SyncMethod, Visibility,
};
pub use context::{Context, GeneratorOperation, LocalFuture};
pub use error::{NativeError, NativeResult, Unwind};
pub use function::{AsyncFn, FunctionEntry, NativeFn, Param, ParamType, SyncFn};
pub use object_handlers::{
    ClassHooks, IteratorFactory, NativeClone, NativeCompare, NativeComparison, NativeCount,
    NativeDimensions, NativeIterator, PropertyProjection, PropertyPurpose,
};
pub use registry::{ConstantValue, Registry};
pub use state::RequestState;

/// The PHP version exposed by the engine and its built-in modules.
pub const PHP_VERSION: &str = "8.5.12";

/// A compile-time linked bundle of PHP functions and constants.
pub trait Extension {
    fn name(&self) -> &'static str;

    /// PHP module name. SAPI services may have no separate PHP module.
    fn php_module(&self) -> Option<&'static str> {
        Some(self.name())
    }

    /// What `phpversion('name')` reports, for a module that has a version of
    /// its own: phpredis says "6.3.0". Modules that ship with PHP report
    /// PHP's version, which is the default.
    fn module_version(&self) -> Option<&'static str> {
        None
    }

    fn register(&self, registry: &mut Registry);
}
