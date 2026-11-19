//! PHP's `serialize($object)` can enter a linked `__serialize()` method.

mod declarations;
mod enums;
pub(crate) mod layout;
mod properties;
mod property_index;

use std::rc::Rc;

use phera_extension_api::serialization::SerializationClass;
use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{Object, Value};

use crate::Vm;
use crate::classes::{Class, Method};

impl Vm {
    pub(crate) async fn run_unserialize_class_callback(
        &mut self,
        name: &str,
    ) -> Result<Option<SerializationClass>, NativeError> {
        let Some(callback) = self
            .ini_get(b"unserialize_callback_func")
            .map(|setting| setting.to_string_lossy())
            .filter(|callback| !callback.is_empty())
        else {
            return Ok(None);
        };
        let callback_value = Value::from(callback.as_str());
        let callee = self.resolve_callable(&callback_value).map_err(|_| {
            NativeError::error(format!(
                "Invalid callback {callback}, function \"{callback}\" not found or invalid \
                 function name"
            ))
        })?;
        self.invoke_resolved_callable(callee, vec![Value::from(name)])
            .await?;
        let result = self
            .lookup_class(name)
            .await?
            .as_ref()
            .map(Self::serialization_class_info);
        if result.is_none() {
            self.warn(format!(
                "unserialize(): Function {callback}() hasn't defined the class it was called for"
            ));
        }
        Ok(result)
    }

    pub(crate) fn serialization_class_info(class: &Rc<Class>) -> SerializationClass {
        SerializationClass {
            name: class.name.clone(),
            forbidden: Self::class_forbids_serialization(class),
            custom: class.is_a("Serializable"),
            has_unserialize: Self::serialization_method(class, "__unserialize").is_some(),
            plain: layout::plain_layout(class),
        }
    }

    pub(crate) fn class_forbids_serialization(class: &Class) -> bool {
        if class.origin == crate::classes::class::ClassOrigin::Closure || class.name.contains('\0')
        {
            return true;
        }
        let mut current = Some(class);
        while let Some(class) = current {
            if class.origin == crate::classes::class::ClassOrigin::Native
                && matches!(
                    class.name.as_str(),
                    "Directory"
                        | "Fiber"
                        | "Generator"
                        | "InternalIterator"
                        | "PDO"
                        | "PDOStatement"
                        | "PDORow"
                        | "Random\\Engine\\Secure"
                        | "ReflectionAttribute"
                        | "ReflectionClass"
                        | "ReflectionClassConstant"
                        | "ReflectionConstant"
                        | "ReflectionExtension"
                        | "ReflectionFunctionAbstract"
                        | "ReflectionGenerator"
                        | "ReflectionFiber"
                        | "ReflectionParameter"
                        | "ReflectionProperty"
                        | "ReflectionReference"
                        | "ReflectionType"
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
