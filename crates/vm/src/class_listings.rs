//! `get_class_methods($object)` lists visible methods in declaration order.

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
            for interface in interfaces_in_php_order(class) {
                result.insert(
                    ArrayKey::str(interface.name.as_str()),
                    interface.name_value.clone(),
                );
            }
        }
        Ok(Value::from(result))
    }
}

/// Every interface of `class`, in the order PHP keeps them: the parent
/// class's list first (reversed for a child without its own implements list),
/// then the interfaces the class names itself, and
/// only then what those interfaces extend, each interface's own list
/// taken back to front.
///
/// ```php
/// interface A {} interface B extends A {} interface C {}
/// class K implements B, C {}
/// class_implements('K');   // B, C, A: the two named ones, then B's parent
/// ```
///
/// A class or interface is worked out after everything it builds on, with
/// a list of work to do in place of recursion.
pub(crate) fn interfaces_in_php_order(class: &Class) -> Vec<&Class> {
    let mut lists: std::collections::HashMap<&str, Vec<&Class>> = Default::default();
    let mut pending = vec![(class, false)];
    while let Some((current, parts_are_ready)) = pending.pop() {
        if lists.contains_key(current.name.as_str()) {
            continue;
        }
        if !parts_are_ready {
            pending.push((current, true));
            pending.extend(current.parent.iter().map(|parent| (parent.as_ref(), false)));
            pending.extend(
                current
                    .interfaces
                    .iter()
                    .map(|named| (named.as_ref(), false)),
            );
            continue;
        }
        let list_of = |part: &Class| lists.get(part.name.as_str()).cloned().unwrap_or_default();
        let mut list = current.parent.as_deref().map(list_of).unwrap_or_default();
        if current.interfaces.is_empty() {
            list.reverse();
        }
        let inherited = list.len();
        for named in &current.interfaces {
            if !list.iter().any(|known| known.name == named.name) {
                list.push(named);
            }
        }
        for index in inherited..list.len() {
            let extended = list_of(list[index]);
            for interface in extended.into_iter().rev() {
                if !list.iter().any(|known| known.name == interface.name) {
                    list.push(interface);
                }
            }
        }
        lists.insert(current.name.as_str(), list);
    }
    lists.remove(class.name.as_str()).unwrap_or_default()
}

/// PHP lists declared methods before inherited methods, retaining source order within each class.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MethodOrder {
    /// Zero denotes the requested class, then each successive parent.
    depth: usize,
    /// User function order or the native registration position.
    declaration: usize,
}

impl Vm {
    /// A method's declaring class determines its position independently of hash table order.
    pub(crate) fn method_listing_order(&self, class: &Class, method: &Method) -> MethodOrder {
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
