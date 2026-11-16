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
    pub(crate) is_readonly: bool,
    pub(crate) allows_dynamic_properties: bool,
    pub(crate) methods: HashMap<String, Rc<Method>>,
    /// Root-first property keys reproduce the layout used by native testing helpers.
    pub(crate) defaults: Vec<PropertyDefault>,
    pub(crate) static_properties: Vec<PropertyDefault>,
    pub(crate) constants: HashMap<String, Rc<LinkedConstant>>,
}

/// Defaults retain their mangled keys so private ancestors do not collide.
#[derive(Clone)]
pub(crate) struct PropertyDefault {
    pub(crate) key: ArrayKey,
    pub(crate) value: Value,
    pub(crate) name: String,
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) initialized: bool,
    pub(crate) initializer: Option<crate::function_table::FunctionRef>,
    pub(crate) ty: Option<phera_bytecode::Type>,
    pub(crate) is_readonly: bool,
    pub(crate) static_initialized: Rc<std::cell::Cell<bool>>,
    pub(crate) value_initialized: Rc<std::cell::Cell<bool>>,
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

/// Lazy constants retain declaring scope and detect recursive initializers before borrowing values.
pub(crate) struct LinkedConstant {
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) is_final: bool,
    pub(crate) initializer: crate::function_table::FunctionRef,
    pub(crate) state: std::cell::RefCell<ConstantState>,
    pub(crate) ty: Option<phera_bytecode::Type>,
}

/// A failed evaluation restores Uninitialized so later accesses can retry.
pub(crate) enum ConstantState {
    Uninitialized,
    Evaluating,
    Ready(Value),
}
