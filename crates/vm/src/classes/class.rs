//! Linked metadata shares inherited methods and defaults while each object owns its properties.

use std::{any::Any, collections::HashMap, rc::Rc};

use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::MethodEntry;
use phera_value::{ArrayKey, ClassHandle, Object, Value};

/// A linked class is immutable; its parent keeps inherited metadata alive.
pub(crate) struct Class {
    pub(crate) name: String,
    pub(crate) parent: Option<Rc<Class>>,
    pub(crate) interfaces: Vec<Rc<Class>>,
    pub(crate) kind: ClassKind,
    pub(crate) is_abstract: bool,
    pub(crate) is_final: bool,
    pub(crate) methods: HashMap<String, Rc<Method>>,
    /// Root-first property keys reproduce the layout used by native testing helpers.
    pub(crate) defaults: Vec<PropertyDefault>,
}

/// Defaults retain their mangled keys so private ancestors do not collide.
#[derive(Clone)]
pub(crate) struct PropertyDefault {
    pub(crate) key: ArrayKey,
    pub(crate) value: Value,
}

/// A method remembers its declaring scope independently of the receiver's class.
pub(crate) struct Method {
    pub(crate) name: String,
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) is_static: bool,
    pub(crate) is_final: bool,
    pub(crate) is_abstract: bool,
    pub(crate) body: MethodBody,
}

/// Native and user methods share argument sends but execute through different handlers.
pub(crate) enum MethodBody {
    User(crate::function_table::FunctionRef),
    Native(MethodEntry),
}

/// Receiver, lexical scope and called class differ under inheritance and static dispatch.
#[derive(Clone)]
pub(crate) struct MethodContext {
    pub(crate) receiver: Option<Rc<Object>>,
    pub(crate) scope: Rc<Class>,
    pub(crate) called: Rc<Class>,
    /// A constructor's return value is discarded in favor of its allocated receiver.
    pub(crate) invocation: Invocation,
}

impl ClassHandle for Class {
    fn name(&self) -> &str {
        &self.name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Class {
    /// Traverse ancestry iteratively so user inheritance never grows the Rust stack.
    pub(crate) fn is_a(&self, name: &str) -> bool {
        let mut pending = vec![self];
        while let Some(class) = pending.pop() {
            if class.name.eq_ignore_ascii_case(name) {
                return true;
            }
            pending.extend(class.parent.iter().map(Rc::as_ref));
            pending.extend(class.interfaces.iter().map(Rc::as_ref));
        }
        false
    }
}

/// Construction discards a method return; an ordinary call forwards it to its caller.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Invocation {
    Constructor,
    Method,
}
