//! PHP's `serialize($object)` can enter a linked `__serialize()` method.

mod declarations;
mod properties;
pub(crate) mod settings;

use std::rc::Rc;

use phera_extension_api::serialization::SerializationClass;
use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{Object, Value};

use crate::Vm;
use crate::classes::{Class, Method};

impl Vm {
    pub(crate) fn serialization_class_info(class: &Class) -> SerializationClass {
        SerializationClass {
            name: class.name.clone(),
            forbidden: Self::class_forbids_serialization(class),
            custom: class.is_a("Serializable"),
            has_unserialize: Self::serialization_method(class, "__unserialize").is_some(),
        }
    }

    pub(crate) fn class_forbids_serialization(class: &Class) -> bool {
        if class.origin == crate::classes::class::ClassOrigin::Closure {
            return true;
        }
        let mut current = Some(class);
        while let Some(class) = current {
            if class.origin == crate::classes::class::ClassOrigin::Native
                && matches!(
                    class.name.as_str(),
                    "Directory"
                        | "InternalIterator"
                        | "PDO"
                        | "PDOStatement"
                        | "PDORow"
                        | "Random\\Engine\\Secure"
                        | "SQLite3"
                        | "SQLite3Result"
                        | "SQLite3Stmt"
                )
            {
                return true;
            }
            current = class.parent.as_deref();
        }
        false
    }

    pub(crate) fn serialization_method<'a>(class: &'a Class, name: &str) -> Option<&'a Rc<Method>> {
        class
            .methods
            .get(name)
            .filter(|method| !method.is_abstract && !method.is_static)
    }

    pub(crate) fn invoke_serialization_method(
        &mut self,
        object: Rc<Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        let Some(class) = self.classes.for_object(&object) else {
            return Box::pin(std::future::ready(Err(internal_error!(
                "serialization class"
            ))));
        };
        let Some(method) = Self::serialization_method(&class, name).cloned() else {
            let error =
                NativeError::error(format!("Call to undefined method {}::{name}()", class.name));
            return Box::pin(std::future::ready(Err(error)));
        };
        self.invoke_magic_method(method, class, object, arguments)
    }

    pub(crate) fn allocate_unserialized_object(&mut self, class: Rc<Class>) -> NativeResult {
        let object = self.classes.allocate(class.clone());
        if let Some(template) = &class.property_template {
            object.update(|properties| *properties = template.clone());
        } else {
            for property in &class.defaults {
                if property.initialized {
                    let value = if let Some(initializer) = &property.initializer {
                        self.evaluate_initializer(initializer, &property.declaring)?
                    } else {
                        property.value.clone()
                    };
                    object.set(property.key.clone(), value);
                }
            }
        }
        if class.is_throwable {
            self.initialize_throwable(&object, crate::exceptions::TraceOrigin::Construction);
        }
        Ok(Value::Object(object))
    }
}
