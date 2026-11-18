//! PHP value model. See `docs/architecture.md`, section 3.

use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

use crate::array::{ArrayKey, PhpArray};
use crate::error::OpError;
use crate::object::{Object, Resource};
use crate::string::PhpString;

#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(PhpString),
    /// Copy-on-write through `Rc::make_mut`.
    Array(Rc<PhpArray>),
    /// Handle semantics: clones share the object.
    Object(Rc<Object>),
    Resource(Rc<Resource>),
    /// A PHP `&` reference. Only found in variable slots and array slots;
    /// operators see through it. The inner value is never itself a
    /// `Reference` (enforced where references are created).
    Reference(Rc<RefCell<Value>>),
}

/// What [`Value::deref`] hands out: either the value itself, or the guarded
/// contents of a `Reference` cell.
pub enum ValueRef<'a> {
    Direct(&'a Value),
    Indirect(Ref<'a, Value>),
}

impl std::ops::Deref for ValueRef<'_> {
    type Target = Value;
    fn deref(&self) -> &Value {
        match self {
            ValueRef::Direct(v) => v,
            ValueRef::Indirect(r) => r,
        }
    }
}

impl Value {
    /// Arrays and objects implementing Traversable satisfy PHP's iterable type.
    pub fn is_iterable(&self) -> bool {
        match self {
            Self::Array(_) => true,
            Self::Object(object) => object.class.is_traversable(),
            _ => false,
        }
    }

    /// Follows a `Reference` to the value it points at; non-reference values
    /// return themselves. The borrow is held through a guard because the
    /// referred value lives in a `RefCell`.
    pub fn deref(&self) -> ValueRef<'_> {
        match self {
            Value::Reference(r) => ValueRef::Indirect(r.borrow()),
            other => ValueRef::Direct(other),
        }
    }

    /// Reads a reference in an operation that can report a borrow conflict to PHP.
    pub fn try_deref(&self) -> Result<ValueRef<'_>, OpError> {
        match self {
            Value::Reference(cell) => cell
                .try_borrow()
                .map(ValueRef::Indirect)
                .map_err(|_| OpError::error("Cannot read a reference while it is being modified")),
            other => Ok(ValueRef::Direct(other)),
        }
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Value::Reference(_))
    }

    /// `get_debug_type`-style name used in error messages.
    pub fn type_name(&self) -> String {
        match &*self.deref() {
            Value::Null => "null".into(),
            Value::Bool(_) => "bool".into(),
            Value::Int(_) => "int".into(),
            Value::Float(_) => "float".into(),
            Value::String(_) => "string".into(),
            Value::Array(_) => "array".into(),
            Value::Object(o) => o.class.name().to_string(),
            Value::Resource(_) => "resource".into(),
            // References always point at non-references, so this is
            // unreachable for well-formed values.
            Value::Reference(_) => "reference".into(),
        }
    }

    /// The name `gettype()` reports.
    pub fn gettype_name(&self) -> &'static str {
        match &*self.deref() {
            Value::Null => "NULL",
            Value::Bool(_) => "boolean",
            Value::Int(_) => "integer",
            Value::Float(_) => "double",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Resource(r) => {
                if r.is_closed() {
                    "resource (closed)"
                } else {
                    "resource"
                }
            }
            Value::Reference(_) => "reference",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(&*self.deref(), Value::Null)
    }
    pub fn is_bool(&self) -> bool {
        matches!(&*self.deref(), Value::Bool(_))
    }
    pub fn is_int(&self) -> bool {
        matches!(&*self.deref(), Value::Int(_))
    }
    pub fn is_float(&self) -> bool {
        matches!(&*self.deref(), Value::Float(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(&*self.deref(), Value::String(_))
    }
    pub fn is_array(&self) -> bool {
        matches!(&*self.deref(), Value::Array(_))
    }
    pub fn is_object(&self) -> bool {
        matches!(&*self.deref(), Value::Object(_))
    }
    pub fn is_resource(&self) -> bool {
        matches!(&*self.deref(), Value::Resource(_))
    }
}

/// Structural equality with `===` semantics: useful for tests and callers
/// that need strict comparison without operator conversion rules.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        crate::ops::compare::strict_equals(self, other, &mut crate::error::NoDiagnostics)
            .map(|v| matches!(v, Value::Bool(true)))
            .unwrap_or(false)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "Null"),
            Value::Bool(b) => write!(f, "Bool({b})"),
            Value::Int(i) => write!(f, "Int({i})"),
            Value::Float(x) => write!(f, "Float({x:?})"),
            Value::String(s) => write!(f, "String({s:?})"),
            Value::Array(a) => write!(f, "Array({a:?})"),
            Value::Object(o) => write!(f, "{o:?}"),
            Value::Resource(r) => write!(f, "{r:?}"),
            Value::Reference(r) => write!(f, "Reference({:?})", r.borrow()),
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}
impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(PhpString::from(s))
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(PhpString::from(s))
    }
}
impl From<Vec<u8>> for Value {
    fn from(b: Vec<u8>) -> Self {
        Value::String(PhpString::from(b))
    }
}
impl From<&[u8]> for Value {
    fn from(b: &[u8]) -> Self {
        Value::String(PhpString::from(b))
    }
}
impl From<PhpString> for Value {
    fn from(s: PhpString) -> Self {
        Value::String(s)
    }
}
impl From<PhpArray> for Value {
    fn from(a: PhpArray) -> Self {
        Value::Array(Rc::new(a))
    }
}
impl From<ArrayKey> for Value {
    fn from(k: ArrayKey) -> Self {
        match k {
            ArrayKey::Int(i) => Value::Int(i),
            ArrayKey::String(s) => Value::String(s),
        }
    }
}
