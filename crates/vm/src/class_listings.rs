//! `get_class_methods($object)` lists visible methods in declaration order.

use std::{collections::HashSet, rc::Rc};

use phera_bytecode::Visibility;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{
    Vm,
    class_intrinsics::ClassFunction,
    classes::{Class, Method},
};

impl Vm {
    /// User-facing property enumeration strips visibility prefixes after checking the current scope.
    pub(crate) fn visible_properties(
        &mut self,
        object: &phera_value::Object,
        purpose: phera_extension_api::PropertyPurpose,
    ) -> Result<phera_value::PhpArray, NativeError> {
        let mut result = phera_value::PhpArray::new();
        for (key, value) in
            phera_extension_api::Context::object_properties(self, object, purpose)?.iter()
        {
            let name = crate::path_update::property_name(&key);
            if self
                .property_key(object, &name)
                .is_ok_and(|resolved| resolved == key)
            {
                result.insert(ArrayKey::str(name), value.clone());
            }
        }
        Ok(result)
    }

    /// Class variables list declaration defaults, including null for uninitialized typed properties.
    pub(crate) async fn class_variables(
        &mut self,
        class: &crate::classes::Class,
        origin: (&str, &[Value]),
    ) -> NativeResult {
        let mut result = phera_value::PhpArray::new();
        let mut ancestry = Vec::new();
        let mut current = Some(class);
        while let Some(class) = current {
            ancestry.push(class.name.as_str());
            current = class.parent.as_deref();
        }
        for declarations in [&class.defaults, &class.static_properties] {
            let mut properties: Vec<_> = declarations.iter().collect();
            properties.sort_by_key(|property| {
                ancestry.iter().position(|name| *name == property.declaring)
            });
            for property in properties {
                let scope = self.current().and_then(|frame| frame.scope());
                let visible = match property.visibility {
                    Visibility::Public => true,
                    Visibility::Private => {
                        scope.is_some_and(|scope| scope.name == property.declaring)
                    }
                    Visibility::Protected => scope.is_some_and(|scope| {
                        scope.is_a(&property.declaring)
                            || self
                                .classes
                                .get(&property.declaring)
                                .is_some_and(|owner| owner.is_a(&scope.name))
                    }),
                };
                let key = ArrayKey::str(property.name.as_str());
                if !visible || result.contains_key(&key) {
                    continue;
                }
                let value = if let Some(initializer) = &property.initializer {
                    loop {
                        match self.evaluate_initializer(initializer, &property.declaring) {
                            Ok(value) => break value,
                            Err(error) => {
                                let miss = crate::autoload::ClassMiss::take(error)?;
                                if self
                                    .lookup_class_from(&miss.name, Some(origin))
                                    .await?
                                    .is_none()
                                {
                                    return Err(miss.error());
                                }
                            }
                        }
                    }
                } else {
                    property.value.deref().clone()
                };
                result.insert(key, value);
            }
        }
        Ok(Value::from(result))
    }

    /// Inherited private declarations only count for objects in their declaring scope.
    pub(crate) fn class_property_exists(
        &self,
        target: &Value,
        class: &crate::classes::Class,
        name: &str,
    ) -> bool {
        class
            .defaults
            .iter()
            .chain(&class.static_properties)
            .any(|property| {
                property.name == name
                    && (property.visibility != Visibility::Private
                        || property.declaring == class.name)
            })
            || matches!(target, Value::Object(object) if
                self.property_key(object, name).is_ok_and(|key| object.has(&key))
            )
    }
}
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
