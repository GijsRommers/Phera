//! Classes written in Rust: `Exception`, `Closure`, `ArrayObject`.
//!
//! An extension describes a class as data. The VM turns the description into
//! one of its own classes at startup, so PHP code can extend it, catch it and
//! call its methods exactly as with a class written in PHP.
//!
//! ```php
//! $date->format('Y-m-d'); // a native method
//! var_dump($date);       // a native property view
//! ```
//!
//! [`ClassEntry`] declares members, [`MethodEntry`] describes call preparation,
//! and [`ClassHooks`] groups optional object operations inherited by subclasses.

use std::rc::Rc;

use phera_value::{Object, PhpString, Value};

use crate::{ClassHooks, ConstantValue, Context, LocalFuture, NativeError, NativeResult, Param};

/// An extension can preserve legacy parsing behavior while keeping its reflected signature.
/// Most methods use Default; Prepared means the hook already checked and coerced arguments.
pub enum MethodPreparation {
    Default,
    Prepared,
    Return(Value),
}

pub type PrepareMethod =
    fn(&mut dyn Context, &[Param], &mut [Value]) -> Result<MethodPreparation, crate::NativeError>;

/// A method that finishes without waiting. `this` is `None` when the method
/// is static.
pub type SyncMethod = fn(&mut dyn Context, Option<&Rc<Object>>, &[Value]) -> NativeResult;

/// An optional no-context handler for a public method with plain positional arguments.
/// A declined call leaves state untouched and is handled by the usual method.
pub type DirectMethod = fn(&Object, &[Option<Value>]) -> Option<Value>;

/// A method that may wait, for I/O or for a PHP callback. See
/// [`crate::AsyncFn`] for why it must never block instead.
pub type AsyncMethod = for<'a> fn(
    &'a mut dyn Context,
    Option<Rc<Object>>,
    Vec<Value>,
) -> LocalFuture<'a, NativeResult>;

/// An async method that forwards names from its variadic arguments into a PHP call.
pub type AsyncNamedMethod = for<'a> fn(
    &'a mut dyn Context,
    Option<Rc<Object>>,
    Vec<Value>,
    &'a [(usize, PhpString)],
) -> LocalFuture<'a, NativeResult>;

#[derive(Clone, Copy)]
pub enum NativeMethod {
    Sync(SyncMethod),
    Async(AsyncMethod),
    AsyncNamed(AsyncNamedMethod),
    /// Declared but without a body: an interface method, or an abstract one.
    Abstract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    /// A class that cannot be instantiated, only extended.
    AbstractClass,
    /// A class that cannot be extended.
    FinalClass,
    Interface,
}

/// Internal signatures can require a return type or give a tentative compatibility warning.
#[derive(Clone, Copy)]
pub enum MethodReturn {
    Required(&'static str),
    Tentative(&'static str),
}

#[derive(Clone, Copy)]
pub struct MethodEntry {
    pub name: &'static str,
    pub params: &'static [Param],
    pub handler: NativeMethod,
    /// Optional argument handling before the ordinary native signature check.
    pub preparation: Option<PrepareMethod>,
    pub direct: Option<DirectMethod>,
    pub visibility: Visibility,
    pub is_static: bool,
    /// Subclasses may not override it.
    pub is_final: bool,
    /// PHP's compatibility contract for an internal method.
    pub return_type: Option<MethodReturn>,
}

impl MethodEntry {
    /// A public instance method, by far the most common kind.
    pub const fn public(name: &'static str, params: &'static [Param], handler: SyncMethod) -> Self {
        MethodEntry {
            name,
            params,
            handler: NativeMethod::Sync(handler),
            preparation: None,
            direct: None,
            visibility: Visibility::Public,
            is_static: false,
            is_final: false,
            return_type: None,
        }
    }

    /// A public instance method that may wait: for a database, a socket, a
    /// timer or a PHP callback. While it waits, the worker thread runs other
    /// requests.
    pub const fn public_async(
        name: &'static str,
        params: &'static [Param],
        handler: AsyncMethod,
    ) -> Self {
        MethodEntry {
            name,
            params,
            handler: NativeMethod::Async(handler),
            preparation: None,
            direct: None,
            visibility: Visibility::Public,
            is_static: false,
            is_final: false,
            return_type: None,
        }
    }

    pub const fn public_async_named(
        name: &'static str,
        params: &'static [Param],
        handler: AsyncNamedMethod,
    ) -> Self {
        MethodEntry {
            name,
            params,
            handler: NativeMethod::AsyncNamed(handler),
            preparation: None,
            direct: None,
            visibility: Visibility::Public,
            is_static: false,
            is_final: false,
            return_type: None,
        }
    }

    /// A method an interface or abstract class requires.
    pub const fn required(name: &'static str, params: &'static [Param]) -> Self {
        MethodEntry {
            name,
            params,
            handler: NativeMethod::Abstract,
            preparation: None,
            direct: None,
            visibility: Visibility::Public,
            is_static: false,
            is_final: false,
            return_type: None,
        }
    }

    pub const fn tentative_return(mut self, ty: &'static str) -> Self {
        self.return_type = Some(MethodReturn::Tentative(ty));
        self
    }

    pub const fn with_preparation(mut self, preparation: PrepareMethod) -> Self {
        self.preparation = Some(preparation);
        self
    }

    pub const fn with_direct(mut self, handler: DirectMethod) -> Self {
        self.direct = Some(handler);
        self
    }

    pub const fn returning(mut self, ty: &'static str) -> Self {
        self.return_type = Some(MethodReturn::Required(ty));
        self
    }

    pub const fn final_(mut self) -> Self {
        self.is_final = true;
        self
    }

    pub const fn static_(mut self) -> Self {
        self.is_static = true;
        self
    }

    pub const fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }
}

/// A type declared on a native instance property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Mixed,
    NullableString,
    NullableClass(&'static str),
}

/// A property every instance starts with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertyEntry {
    pub name: &'static str,
    pub default: Option<ConstantValue>,
    pub ty: Option<PropertyType>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_readonly: bool,
}

impl PropertyEntry {
    pub const fn new(name: &'static str, default: ConstantValue, visibility: Visibility) -> Self {
        PropertyEntry {
            name,
            default: Some(default),
            ty: None,
            visibility,
            is_static: false,
            is_readonly: false,
        }
    }

    /// A typed property with no initial value, like PHP's `public int $id;`.
    pub const fn typed(name: &'static str, ty: PropertyType, visibility: Visibility) -> Self {
        Self {
            name,
            default: None,
            ty: Some(ty),
            visibility,
            is_static: false,
            is_readonly: false,
        }
    }

    pub const fn with_default(mut self, default: ConstantValue) -> Self {
        self.default = Some(default);
        self
    }

    pub const fn readonly(mut self) -> Self {
        self.is_readonly = true;
        self
    }
}

/// Instance access for native properties whose values live outside ordinary storage.
pub enum NativePropertyRead {
    Read,
    Update,
    Quiet,
    Isset,
    Exists,
}

/// A mutation reaching a native property before ordinary storage is changed.
pub enum NativePropertyMutation<'a> {
    Assign(&'a Value),
    Bind,
    Reference,
    Unset,
    Indirect,
}

/// Return `None` to use ordinary storage. For existence checks, any `Some` counts.
pub type ReadNativeProperty =
    fn(&mut dyn Context, &Object, &str, NativePropertyRead) -> Result<Option<Value>, NativeError>;

/// Return `true` when the mutation has already been handled.
pub type MutateNativeProperty =
    fn(&mut dyn Context, &Object, &str, NativePropertyMutation<'_>) -> Result<bool, NativeError>;

pub struct NativePropertyAccess {
    pub read: ReadNativeProperty,
    pub mutate: MutateNativeProperty,
}

/// A class or interface provided by an extension.
#[derive(Clone, Copy)]
pub struct ClassEntry {
    pub name: &'static str,
    pub kind: ClassKind,
    /// Must be registered before this class.
    pub parent: Option<&'static str>,
    pub interfaces: &'static [&'static str],
    /// Public, overridable values whose PHP types are inferred from their registered values.
    /// Linked immediately so `PDO::FETCH_ASSOC` also works in defaults.
    /// For example, PHP may declare `function rows($mode = PDO::FETCH_ASSOC) {}`.
    pub constants: &'static [(&'static str, ConstantValue)],
    /// Names and PHP diagnostic messages, alongside the constants' declared values.
    pub deprecated_constants: &'static [(&'static str, &'static str)],
    pub properties: &'static [PropertyEntry],
    pub methods: &'static [MethodEntry],
    /// Operations for state held outside ordinary PHP properties.
    pub hooks: Option<&'static ClassHooks>,
}

impl ClassEntry {
    /// An ordinary class with nothing in it yet; fill in the fields that
    /// differ with struct update syntax:
    /// `ClassEntry { parent: Some("Exception"), ..ClassEntry::class("RuntimeException") }`.
    pub const fn class(name: &'static str) -> Self {
        ClassEntry {
            name,
            kind: ClassKind::Class,
            parent: None,
            interfaces: &[],
            constants: &[],
            deprecated_constants: &[],
            properties: &[],
            methods: &[],
            hooks: None,
        }
    }

    pub const fn interface(name: &'static str) -> Self {
        ClassEntry {
            kind: ClassKind::Interface,
            ..ClassEntry::class(name)
        }
    }
}
