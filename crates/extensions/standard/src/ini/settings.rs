//! Runtime configuration changes are local to a request.
//!
//! ```php
//! ini_set('default_charset', 'ISO-8859-1'); // previous encoding
//! ini_get('default_charset');             // "ISO-8859-1"
//! ini_restore('default_charset');
//! ```

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpString,
    Registry, Value, args, ini, value,
};

const OPTION: Param = Param::new("option", ParamType::String);
const SET: &[Param] = &[OPTION, Param::new("value", ParamType::Mixed)];
const ALL: &[Param] = &[
    Param::new("extension", ParamType::String)
        .with_default(ConstantValue::Null)
        .nullable(),
    Param::new("details", ParamType::Bool).with_default(ConstantValue::Bool(true)),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, access) in [
        ("INI_USER", 1_i64),
        ("INI_PERDIR", 2),
        ("INI_SYSTEM", 4),
        ("INI_ALL", 7),
    ] {
        registry.constant(name, access);
    }
    for entry in [
        FunctionEntry::sync("ini_get", &[OPTION], get),
        FunctionEntry::sync("ini_set", SET, set),
        FunctionEntry::sync("ini_alter", SET, alter),
        FunctionEntry::sync("ini_restore", &[OPTION], restore),
        FunctionEntry::sync("ini_get_all", ALL, list),
        FunctionEntry::sync(
            "ini_parse_quantity",
            const { &[Param::new("shorthand", ParamType::String)] },
            quantity,
        ),
        FunctionEntry::sync("php_ini_loaded_file", &[], no_ini),
        FunctionEntry::sync("php_ini_scanned_files", &[], no_ini),
        FunctionEntry::sync("get_include_path", &[], get_include_path),
        FunctionEntry::sync(
            "set_include_path",
            const { &[Param::new("include_path", ParamType::String)] },
            set_include_path,
        ),
    ] {
        registry.function(entry);
    }
}

fn get(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(result(context.ini_get(args::bytes(arguments, 0))))
}

fn set(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change(context, arguments, "ini_set")
}

fn alter(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change(context, arguments, "ini_alter")
}

fn change(context: &mut dyn Context, arguments: &[Value], name: &str) -> NativeResult {
    let value = args::value(arguments, 1).deref().clone();
    if !matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
    ) {
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #2 ($value) must be of type string|int|float|bool|null",
        )));
    }
    let value = value::to_php_string(&value, context.diagnostics())?;
    Ok(result(context.ini_set(
        name,
        args::bytes(arguments, 0),
        value,
    )?))
}

fn restore(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    context.ini_restore(args::bytes(arguments, 0))?;
    Ok(Value::Null)
}

fn list(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    context.ini_list(
        args::string(arguments, 0),
        args::bool(arguments, 1).unwrap_or(true),
    )
}

fn quantity(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (number, warning) = ini::parse_quantity(args::bytes(arguments, 0));
    if let Some(warning) = warning {
        context.warning(warning);
    }
    Ok(Value::Int(number))
}

fn no_ini(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
}

fn get_include_path(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(context.include_path()))
}

fn set_include_path(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let path = args::string(arguments, 0).cloned().unwrap_or_default();
    Ok(result(context.ini_set(
        "set_include_path",
        b"include_path",
        path,
    )?))
}

fn result(value: Option<PhpString>) -> Value {
    value.map_or(Value::Bool(false), Value::String)
}
