//! `get_class_methods($object)` lists visible methods in declaration order.

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::NativeResult;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{
    Vm,
    class_intrinsics::ClassFunction,
    classes::{Class, Method},
};

impl Vm {
    pub(crate) fn class_methods(&self, class: &Class) -> NativeResult {
        let mut methods: Vec<_> = class.methods.values().collect();
        methods.sort_by_key(|method| self.method_listing_order(class, method));
        let mut result = PhpArray::new();
        for method in methods {
            if self.check_method_visibility(method).is_ok() {
                result.push(Value::from(method.name.clone()))?;
            }
        }
        Ok(Value::from(result))
    }

    /// A subclass lists inherited interfaces before its own additions.
    pub(crate) fn class_relations(&self, operation: ClassFunction, class: &Class) -> NativeResult {
        let mut result = PhpArray::new();
        let mut ancestors = Vec::new();
        let mut parent = class.parent.as_deref();
        while let Some(class) = parent {
            ancestors.push(class);
            parent = class.parent.as_deref();
        }
        if matches!(operation, ClassFunction::ClassParents) {
            for parent in ancestors {
                result.insert(
                    ArrayKey::str(parent.name.as_str()),
                    parent.name_value.clone(),
                );
            }
        } else {
            let mut seen = HashSet::new();
            for current in ancestors.into_iter().rev().chain([class]) {
                let mut pending: Vec<_> = current.interfaces.iter().rev().map(Rc::as_ref).collect();
                while let Some(interface) = pending.pop() {
                    if seen.insert(&interface.name) {
                        result.insert(
                            ArrayKey::str(interface.name.as_str()),
                            interface.name_value.clone(),
                        );
                        pending.extend(interface.interfaces.iter().rev().map(Rc::as_ref));
                    }
                }
            }
        }
        Ok(Value::from(result))
    }
}

/// PHP lists declared methods before inherited methods, retaining source order within each class.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct MethodOrder {
    /// Zero denotes the requested class, then each successive parent.
    depth: usize,
    /// User function order or the native registration position.
    declaration: usize,
}

impl Vm {
    /// A method's declaring class determines its position independently of hash table order.
    fn method_listing_order(&self, class: &Class, method: &Method) -> MethodOrder {
        let mut ancestor = Some(class);
        let mut depth = 0;
        while let Some(class) = ancestor {
            if class.name == method.declaring {
                break;
            }
            depth += 1;
            ancestor = class.parent.as_deref();
        }
        let declaration = match &method.body {
            crate::classes::MethodBody::User(reference) => reference.index(),
            crate::classes::MethodBody::Closure(operation) => {
                crate::closure_methods::ClosureMethod::entries()
                    .iter()
                    .position(|(entry, _)| entry == operation)
                    .unwrap_or(0)
            }
            crate::classes::MethodBody::Native(_) => self
                .registry
                .classes()
                .find(|class| class.name == method.declaring)
                .and_then(|class| {
                    class
                        .methods
                        .iter()
                        .position(|entry| entry.name == method.name)
                })
                .unwrap_or(0),
        };
        MethodOrder { depth, declaration }
    }
}
