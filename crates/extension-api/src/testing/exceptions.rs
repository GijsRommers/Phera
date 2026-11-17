//! Inspect native failures as PHP objects without starting a VM or inventing a call-site trace.

use std::rc::Rc;

use phera_value::{ArrayKey, Object, Value};

use crate::{NativeError, Registry, Visibility};

/// Materialize declared defaults, code and properties so extension tests can inspect a throw.
/// For example, assert what PHP reads with `$e->getCode()` and `$e->errorInfo[0]` after a failure.
/// File, line and trace keep their declared defaults because only the VM knows the PHP call site.
/// Existing PHP unwinds are returned untouched, just as they are at a native callback boundary.
pub fn materialize_exception(
    registry: &Registry,
    error: NativeError,
) -> Result<Rc<Object>, NativeError> {
    let (class, message, code, properties) = error.into_exception_parts()?;
    let object = super::instantiate(registry, class).ok_or_else(|| {
        NativeError::error(format!(
            "Phera internal error: native exception class {class} is not registered"
        ))
    })?;
    object.set(Object::protected_key("message"), Value::from(message));
    object.set(Object::protected_key("code"), code);
    for (name, value) in properties.iter() {
        let mut next = registry.get_class(class);
        let mut key = None;
        while let Some(entry) = next {
            if let Some(property) = entry
                .properties
                .iter()
                .find(|property| !property.is_static && *name == ArrayKey::str(property.name))
            {
                key = Some(match property.visibility {
                    Visibility::Public => ArrayKey::str(property.name),
                    Visibility::Protected => Object::protected_key(property.name),
                    Visibility::Private => Object::private_key(entry.name, property.name),
                });
                break;
            }
            next = entry.parent.and_then(|parent| registry.get_class(parent));
        }
        let key = key.ok_or_else(|| {
            let name = match name {
                ArrayKey::String(name) => name.to_string_lossy(),
                ArrayKey::Int(name) => name.to_string(),
            };
            NativeError::error(format!(
                "Phera internal error: native exception property {class}::${name} is not declared"
            ))
        })?;
        object.set(key, value.clone());
    }
    Ok(object)
}

#[cfg(test)]
mod tests {
    use crate::{ClassEntry, ConstantValue, NativeError, PropertyEntry, Registry, Visibility};
    use phera_value::{ArrayKey, Object, Value};

    use super::materialize_exception;

    fn registry() -> Registry {
        const ROOT_PROPERTIES: &[PropertyEntry] = &[
            PropertyEntry::new("code", ConstantValue::Int(0), Visibility::Protected),
            PropertyEntry::new("details", ConstantValue::Null, Visibility::Private),
        ];
        const DRIVER_PROPERTIES: &[PropertyEntry] = &[PropertyEntry::new(
            "errorInfo",
            ConstantValue::Null,
            Visibility::Public,
        )];
        let mut registry = Registry::new();
        registry.class(ClassEntry {
            properties: ROOT_PROPERTIES,
            ..ClassEntry::class("Exception")
        });
        registry.class(ClassEntry {
            parent: Some("Exception"),
            properties: DRIVER_PROPERTIES,
            ..ClassEntry::class("DriverException")
        });
        registry
    }

    #[test]
    fn native_payload_keeps_string_code_and_declared_properties() {
        let information = Value::from(
            [
                (ArrayKey::Int(0), Value::from("42P01")),
                (ArrayKey::Int(1), Value::Int(7)),
            ]
            .into_iter()
            .collect::<phera_value::PhpArray>(),
        );
        fn failure(_context: &mut dyn crate::Context, _arguments: &[Value]) -> crate::NativeResult {
            let mut information = phera_value::PhpArray::new();
            information.push(Value::from("42P01")).unwrap();
            information.push(Value::Int(7)).unwrap();
            Err(NativeError::throw("DriverException", "missing")
                .with_property("errorInfo", Value::Null)
                .with_code("42P01")
                .with_property("errorInfo", Value::from(information))
                .with_property("details", Value::from("private")))
        }
        let mut registry = registry();
        registry.function(crate::FunctionEntry::sync("failure", &[], failure));
        let mut context = crate::testing::TestContext::default();
        let error = crate::testing::call(&registry, &mut context, "failure", vec![]).unwrap_err();
        let object = materialize_exception(&registry, error).unwrap();
        assert_eq!(
            object.get(&Object::protected_key("code")),
            Some(Value::from("42P01"))
        );
        assert_eq!(object.get(&ArrayKey::str("errorInfo")), Some(information));
        assert_eq!(
            object.get(&Object::private_key("Exception", "details")),
            Some(Value::from("private"))
        );
        assert!(!object.has(&ArrayKey::str("details")));
    }

    #[test]
    fn old_integer_constructors_and_integer_builder_keep_working() {
        for (error, expected_code) in [
            NativeError::throw("DriverException", "message"),
            NativeError::throw_with_code("DriverException", "message", 7),
            NativeError::throw("DriverException", "message").with_code(9_i64),
        ]
        .into_iter()
        .zip([0, 7, 9])
        {
            let object = materialize_exception(&registry(), error).unwrap();
            assert_eq!(
                object.get(&Object::protected_key("code")),
                Some(Value::Int(expected_code))
            );
        }
    }

    #[test]
    fn undeclared_properties_and_invalid_codes_report_extension_bugs() {
        let error = NativeError::throw("DriverException", "m").with_property("typo", Value::Null);
        let error = materialize_exception(&registry(), error).unwrap_err();
        let (_, message, _, _) = error.into_exception_parts().unwrap();
        assert_eq!(
            message,
            "Phera internal error: native exception property DriverException::$typo is not declared"
        );
        let error = NativeError::throw("DriverException", "m").with_code(false);
        let (_, message, _, _) = error.into_exception_parts().unwrap();
        assert_eq!(
            message,
            "Phera internal error: native exception code must be int or string"
        );
    }

    #[test]
    fn callback_unwinds_are_preserved_by_builders_and_materialization() {
        let error = NativeError::Unwind(crate::Unwind(Box::new(42_u32)))
            .with_code("42P01")
            .with_property("errorInfo", Value::Null);
        let NativeError::Unwind(payload) = materialize_exception(&registry(), error).unwrap_err()
        else {
            panic!("callback unwind expected");
        };
        assert_eq!(payload.0.downcast_ref::<u32>(), Some(&42));
    }
}
