//! PHP signatures and the classes returned by the mysqli API.

mod connection;
mod dispatch;
mod result;
mod statement_dispatch;
mod statements;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, MethodEntry, NativeClone,
    NativeError, NativePropertyAccess, NativePropertyMutation, NativeResult, Object, Param,
    ParamType, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};
use std::rc::Rc;

use crate::{connection::properties, errors};

pub fn register(registry: &mut Registry) {
    connection::register(registry);
    result::register(registry);
    statements::register(registry);
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks: Some(
            const {
                &ClassHooks {
                    property_access: Some(&DRIVER_PROPERTIES),
                    ..ClassHooks::EMPTY
                }
            },
        ),
        ..ClassEntry::class("mysqli_driver")
    });
    registry.class(ClassEntry {
        parent: Some("RuntimeException"),
        kind: ClassKind::FinalClass,
        properties: const {
            &[
                PropertyEntry::typed("sqlstate", PropertyType::String, Visibility::Protected)
                    .with_default(ConstantValue::String("00000")),
            ]
        },
        methods: const {
            &[
                MethodEntry::public("getSqlState", &[], errors::exception_state)
                    .returning("string"),
            ]
        },
        ..ClassEntry::class("mysqli_sql_exception")
    });
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        properties: const {
            &[
                PropertyEntry::typed("message", PropertyType::String, Visibility::Public),
                PropertyEntry::typed("sqlstate", PropertyType::String, Visibility::Public),
                PropertyEntry::typed("errno", PropertyType::Int, Visibility::Public),
            ]
        },
        methods: const {
            &[
                MethodEntry::public("__construct", &[], private_constructor)
                    .with_visibility(Visibility::Private),
                MethodEntry::public("next", &[], crate::connection::commands::next_warning)
                    .returning("bool"),
            ]
        },
        ..ClassEntry::class("mysqli_warning")
    });
    registry.function(phera_extension_api::FunctionEntry::sync(
        "mysqli_report",
        const { &[Param::new("flags", ParamType::Int)] },
        report,
    ));
}

fn report(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    errors::set_mode(context, args::int(values, 0).unwrap_or_default());
    Ok(Value::Bool(true))
}

fn private_constructor(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

static DRIVER_PROPERTIES: NativePropertyAccess = NativePropertyAccess {
    read: |context, _, name, _| {
        Ok(match name {
            "client_info" => Some(Value::from(properties::CLIENT_INFO)),
            "client_version" => Some(Value::Int(properties::CLIENT_VERSION)),
            "driver_version" => {
                context.deprecated("The driver_version property is deprecated".into());
                Some(Value::Int(101009))
            }
            "report_mode" => Some(Value::Int(errors::mode(context))),
            _ => None,
        })
    },
    mutate: |context, _, name, operation| {
        if name == "report_mode" {
            if let NativePropertyMutation::Assign(value) = operation {
                let mode = phera_extension_api::value::to_int(value, context.diagnostics())?;
                errors::set_mode(context, mode);
                return Ok(true);
            }
            return Err(NativeError::error(
                "Cannot modify mysqli_driver::$report_mode",
            ));
        }
        Ok(false)
    },
};

const MYSQLI_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    properties: Some(properties::view),
    property_access: Some(&properties::ACCESS),
    ..ClassHooks::EMPTY
};
