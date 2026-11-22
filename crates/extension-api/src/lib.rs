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
pub mod native_parameters;
mod object_handlers;
mod registry;
mod request_references;
pub use request_references::register_cycle_fork;
pub mod serialization;
mod state;
pub mod testing;
pub mod traversal;
pub mod web_log;
mod working_directory;

pub use phera_bytecode::{BinaryOp as NativeBinaryOp, UnaryOp as NativeUnaryOp};
pub use phera_source::{CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite};
pub use phera_value as value;
pub use phera_value::ObjectCast as NativeCastKind;
pub use phera_value::{ArrayKey, Diagnostics, Object, PhpArray, PhpString, Value};

pub use arguments::{args, prepare_arguments, prepare_call, prepare_single_argument};
pub use class::{
    AsyncMethod, ClassEntry, ClassKind, DirectMethod, MethodEntry, MethodPreparation, MethodReturn,
    NativeMethod, NativePropertyAccess, NativePropertyMutation, NativePropertyRead,
    NativeStringProperty, PrepareMethod, PropertyEntry, PropertyType, ReadyMethod, SyncMethod,
    Visibility, WindowMethod,
};
pub use context::{Context, GeneratorOperation, LocalFuture, PropertyView, RequestCallback};
mod property_view_request;
pub use error::{NativeError, NativeResult, Unwind};
pub use function::{
    AsyncFn, FunctionEntry, NativeFn, NativeId, NativeNumericParameter, Param, ParamType, SyncFn,
    SyncSelector, deprecated_method,
};
pub use object_handlers::{
    ClassHooks, DimensionReferenceMode, DirectDimensions, IteratorFactory, NativeBinary,
    NativeClone, NativeCompare, NativeComparison, NativeCount, NativeCycleTrace, NativeDimensions,
    NativeForkHook, NativeIterator, NativeRelease, PropertyProjection, PropertyPurpose, fork_clone,
    fork_refused_handle,
};
pub use property_view_request::{PropertyViewContinuation, PropertyViewRequest};
pub use registry::{BundledSource, ConstantValue, Registry, ResourceFork, folded_name};
pub use state::{ProcessCancellation, RequestState, StateFork, copy_state, fresh_state};
pub use working_directory::WorkingDirectory;

/// The iconv errors that PHP reports differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconvFailure {
    UnknownConversion,
    IncompleteCharacter,
    IllegalCharacter,
}

/// Byte conversion for `iconv()` and `convert.iconv.*`, without PHP diagnostics.
pub type IconvConverter = fn(&[u8], &str, &str) -> Result<Vec<u8>, IconvFailure>;

/// The PHP version exposed by the engine and its built-in modules.
pub const PHP_VERSION: &str = "8.5.13";

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

pub mod fiber;
pub mod ini;
pub mod reflection;
pub mod zend_sort;
