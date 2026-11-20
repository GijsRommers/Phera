//! Optional operations implemented by Rust classes.
//!
//! ```php
//! $copy = clone $date;       // copies the native calendar state
//! $rows = (array) $date;    // projects the date fields for an array cast
//! ```

use std::rc::Rc;

use crate::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value,
    traversal::IteratorStep,
};

/// PHP's three-way comparison. Uncomparable pairs fail every ordering test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeComparison {
    Default,
    Ordered(std::cmp::Ordering),
    Uncomparable,
}

/// A native object's property view can differ by consuming operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyPurpose {
    Debug,
    ArrayCast,
    Export,
    Json,
    ObjectVars,
    Foreach,
    Comparison,
    Serialize,
}

/// Native classes may copy their state or explicitly forbid cloning.
#[derive(Clone, Copy)]
pub enum NativeClone {
    Copy(fn(&Rc<Object>, &Object) -> Result<(), NativeError>),
    /// Copy state that must keep a weak reference to the destination object.
    CopyWithHandle(fn(&Rc<Object>, &Rc<Object>) -> Result<(), NativeError>),
    Forbidden,
}

impl NativeClone {
    /// Register this for values such as date/time data that own cloneable state.
    pub const fn copy<T: Clone + 'static>() -> Self {
        Self::Copy(copy_state::<T>)
    }
}

fn copy_state<T: Clone + 'static>(
    source: &Rc<Object>,
    destination: &Object,
) -> Result<(), NativeError> {
    let state = source
        .native
        .try_borrow()
        .map_err(|_| NativeError::error("Native clone source is already borrowed"))?
        .as_ref()
        .and_then(|data| data.downcast_ref::<T>())
        .cloned();
    if let Some(state) = state {
        *destination
            .native
            .try_borrow_mut()
            .map_err(|_| NativeError::error("Native clone destination is already borrowed"))? =
            Some(Box::new(state));
    }
    Ok(())
}

/// A separate cursor is constructed for each traversal of a native object.
pub trait NativeIterator {
    fn rewind(&mut self, context: &mut dyn Context) -> Result<(), NativeError>;
    fn advance(&mut self, context: &mut dyn Context) -> Result<(), NativeError>;
    fn valid(&mut self, context: &mut dyn Context) -> Result<bool, NativeError>;
    fn current(&mut self, context: &mut dyn Context) -> NativeResult;
    fn key(&mut self, context: &mut dyn Context) -> NativeResult;

    /// An opted-in step can wait for I/O before its synchronous cursor operation.
    fn needs_input(&self, _operation: IteratorStep) -> bool {
        false
    }

    fn read_input<'a>(
        &'a mut self,
        _context: &'a mut dyn Context,
    ) -> LocalFuture<'a, Result<(), NativeError>> {
        Box::pin(async { Ok(()) })
    }

    /// Read a foreach entry synchronously, advancing first after the first successful read.
    /// Returning None leaves the cursor untouched so the VM can run the ordinary steps.
    fn next_entry(&mut self, _with_key: bool) -> Option<(Value, Option<Value>)> {
        None
    }
}

pub type IteratorFactory =
    fn(&mut dyn Context, Rc<Object>) -> Result<Box<dyn NativeIterator>, NativeError>;

/// Whether an indirect write may initialize a missing element.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DimensionReferenceMode {
    Write,
    ReadWrite,
    Unset,
}

pub type DimensionReference =
    fn(&mut dyn Context, &Object, Value, DimensionReferenceMode) -> NativeResult;
pub type BindDimension = fn(&mut dyn Context, &Object, Value, Value) -> Result<(), NativeError>;

/// Direct accesses can borrow register operands and complete without a context.
/// A declined access leaves state untouched and uses the ordinary handlers.
#[derive(Clone, Copy)]
pub struct DirectDimensions {
    pub read: fn(&Object, &Value) -> Option<Value>,
    pub write: fn(&Object, &Value, &Value) -> bool,
}

/// ArrayAccess's four methods can be serviced directly by native state.
#[derive(Clone, Copy)]
pub struct NativeDimensions {
    /// Other methods the fast path depends on retaining their native implementation.
    pub fallback_if_overridden: &'static [&'static str],
    pub get: fn(&mut dyn Context, &Object, Value) -> NativeResult,
    pub exists: fn(&mut dyn Context, &Object, Value) -> Result<bool, NativeError>,
    pub set: fn(&mut dyn Context, &Object, Value, Value) -> Result<(), NativeError>,
    pub unset: fn(&mut dyn Context, &Object, Value) -> Result<(), NativeError>,
    /// Returns a backing reference cell for an indirect write or reference binding.
    pub reference: Option<DimensionReference>,
    /// Makes an existing reference cell the backing storage for one element.
    pub bind: Option<BindDimension>,
    pub direct: Option<DirectDimensions>,
}

pub type NativeCompare = fn(
    &Object,
    &Object,
    &mut dyn crate::Diagnostics,
) -> Result<NativeComparison, crate::value::OpError>;
pub type PropertyProjection =
    fn(&mut dyn Context, &Object, PropertyPurpose) -> Result<PhpArray, NativeError>;
pub type NativeCount = fn(&mut dyn Context, &Object) -> Result<i64, NativeError>;
pub type MethodForwarder = fn(&Object) -> Result<Option<Rc<Object>>, NativeError>;

/// A native operand can handle an operator or decline ordinary PHP dispatch.
pub type NativeBinary = fn(
    &mut dyn Context,
    crate::NativeBinaryOp,
    &Value,
    &Value,
) -> Result<Option<Value>, NativeError>;

pub type NativeUnary =
    fn(&mut dyn Context, crate::NativeUnaryOp, &Object) -> Result<Option<Value>, NativeError>;
pub type NativeCast = fn(&Object, crate::NativeCastKind) -> Option<Value>;

/// Copies an object's native state for a fork of a snapshot: every PHP value
/// inside goes through the [`phera_value::Fork`] (`fork.value(...)`), so that
/// objects inside are replaced by their copies. Returns why the object
/// cannot be copied instead (an open connection, a running query).
///
/// ```php
/// $stack = new SplStack;
/// $stack->push(new Job);
/// Phera\snapshot();   // each request gets its own stack with its own Job
/// ```
pub type NativeForkHook = fn(
    &Object,
    &dyn std::any::Any,
    &mut phera_value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String>;

#[derive(Clone, Copy, Default)]
pub struct ClassHooks {
    /// The error for script allocation of a factory-only native object.
    pub construction_error: Option<&'static str>,
    pub legacy_unserialize: Option<crate::serialization::NativeLegacyUnserialize>,
    pub clone: Option<NativeClone>,
    pub compare: Option<NativeCompare>,
    pub binary: Option<NativeBinary>,
    pub unary: Option<NativeUnary>,
    pub cast: Option<NativeCast>,
    /// Native state that cannot be represented by PHP's serialization format.
    pub forbid_serialization: bool,
    /// An exception of this class (or a subclass) shows no argument values in
    /// its trace: they may be keys or passwords.
    pub redact_trace_arguments: bool,
    /// Whole-object enumeration, with the consuming operation supplied as a purpose.
    pub properties: Option<PropertyProjection>,
    /// Direct read, mutation or presence check for one virtual property.
    pub property_access: Option<&'static crate::NativePropertyAccess>,
    pub iterator: Option<IteratorFactory>,
    /// Cursor creation for foreach by reference. Its current values must be reference cells.
    pub iterator_by_reference: Option<IteratorFactory>,
    /// Additional methods a native cursor relies on, beyond the iterator protocol.
    pub iterator_fallback_if_overridden: &'static [&'static str],
    pub dimensions: Option<NativeDimensions>,
    pub count: Option<NativeCount>,
    /// Resolve an otherwise missing instance method on a wrapped receiver.
    pub forward_method: Option<MethodForwarder>,
    /// Copies the native state for a fork; `None` means the class keeps no
    /// state a fork could copy, and an object with state refuses.
    pub fork: Option<NativeForkHook>,
}

impl ClassHooks {
    pub const EMPTY: Self = Self {
        construction_error: None,
        legacy_unserialize: None,
        clone: None,
        compare: None,
        binary: None,
        unary: None,
        cast: None,
        forbid_serialization: false,
        redact_trace_arguments: false,
        properties: None,
        property_access: None,
        iterator: None,
        iterator_by_reference: None,
        iterator_fallback_if_overridden: &[],
        dimensions: None,
        count: None,
        forward_method: None,
        fork: None,
    };
}

/// The fork hook of a class whose native state is plain data with no PHP
/// value inside (a number, a formatter's settings): the fork gets a clone.
///
/// ```php
/// $money = new NumberFormatter('nl_NL', NumberFormatter::CURRENCY);
/// Phera\snapshot();   // each request formats with its own copy
/// ```
pub fn fork_clone<T: Clone + 'static>(
    _object: &Object,
    state: &dyn std::any::Any,
    _fork: &mut phera_value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    state
        .downcast_ref::<T>()
        .map(|state| Box::new(state.clone()) as Box<dyn std::any::Any>)
        .ok_or_else(|| "its built-in state is not initialized".into())
}

/// The fork hook of a class whose objects hold a connection, a transfer or
/// a compression in progress: one of those belongs to one request.
pub fn fork_refused_handle(
    _object: &Object,
    _state: &dyn std::any::Any,
    _fork: &mut phera_value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    Err("a connection, transfer or compression context made before the call belongs to one request; create it after Phera\\snapshot()".into())
}
