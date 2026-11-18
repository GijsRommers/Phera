//! Optional operations implemented by Rust classes.
//!
//! ```php
//! $copy = clone $date;       // copies the native calendar state
//! $rows = (array) $date;    // projects the date fields for an array cast
//! ```

use std::rc::Rc;

use crate::{Context, NativeError, NativeResult, Object, PhpArray, Value};

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
    Copy(fn(&Object, &Object) -> Result<(), NativeError>),
    Forbidden,
}

/// A separate cursor is constructed for each traversal of a native object.
pub trait NativeIterator {
    fn rewind(&mut self, context: &mut dyn Context) -> Result<(), NativeError>;
    fn advance(&mut self, context: &mut dyn Context) -> Result<(), NativeError>;
    fn valid(&mut self, context: &mut dyn Context) -> Result<bool, NativeError>;
    fn current(&mut self, context: &mut dyn Context) -> NativeResult;
    fn key(&mut self, context: &mut dyn Context) -> NativeResult;
}

pub type IteratorFactory =
    fn(&mut dyn Context, Rc<Object>) -> Result<Box<dyn NativeIterator>, NativeError>;

/// ArrayAccess's four methods can be serviced directly by native state.
#[derive(Clone, Copy)]
pub struct NativeDimensions {
    pub get: fn(&mut dyn Context, &Object, Value) -> NativeResult,
    pub exists: fn(&mut dyn Context, &Object, Value) -> Result<bool, NativeError>,
    pub set: fn(&mut dyn Context, &Object, Value, Value) -> Result<(), NativeError>,
    pub unset: fn(&mut dyn Context, &Object, Value) -> Result<(), NativeError>,
}

pub type NativeCompare = fn(
    &Object,
    &Object,
    &mut dyn crate::Diagnostics,
) -> Result<NativeComparison, crate::value::OpError>;
pub type PropertyProjection =
    fn(&mut dyn Context, &Object, PropertyPurpose) -> Result<PhpArray, NativeError>;
pub type NativeCount = fn(&mut dyn Context, &Object) -> Result<i64, NativeError>;

#[derive(Clone, Copy, Default)]
pub struct ClassHooks {
    pub clone: Option<NativeClone>,
    pub compare: Option<NativeCompare>,
    pub properties: Option<PropertyProjection>,
    pub iterator: Option<IteratorFactory>,
    pub dimensions: Option<NativeDimensions>,
    pub count: Option<NativeCount>,
}

impl ClassHooks {
    pub const EMPTY: Self = Self {
        clone: None,
        compare: None,
        properties: None,
        iterator: None,
        dimensions: None,
        count: None,
    };
}
