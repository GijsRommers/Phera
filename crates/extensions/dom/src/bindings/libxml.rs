//! Keep libxml's internal error state within the current PHP request.
//!
//! ```php
//! libxml_use_internal_errors(true);
//! $document->loadXML('<broken>');
//! echo libxml_get_last_error()->message;
//! ```

mod constants;
mod loaders;
pub(super) use constants::register_dom_constants;

use phera_extension_api::value::ArrayKey;
use phera_extension_api::{
    ClassEntry, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, PropertyEntry,
    PropertyType, Registry, Value, Visibility, args,
};

use crate::document::ParseError;

#[derive(Default)]
pub(crate) struct Errors {
    internal: bool,
    last: Option<ParseError>,
    all: Vec<ParseError>,
}

pub(super) fn publish(
    context: &mut dyn Context,
    function: &str,
    source: &str,
    diagnostics: Vec<ParseError>,
    flags: i64,
) {
    for mut error in diagnostics {
        error.file = source.into();
        if record(context, &error) && (flags & if error.level == 1 { 64 } else { 32 }) == 0 {
            let file = if source.is_empty() { "Entity" } else { source };
            context.warning(format!(
                "{function}(): {} in {file}, line: {}",
                error.message.trim_end(),
                error.line
            ));
        }
    }
}

pub(super) fn publish_with_source(
    context: &mut dyn Context,
    function: &str,
    source: &str,
    input: &[u8],
    diagnostics: Vec<ParseError>,
    flags: i64,
) {
    let input = String::from_utf8_lossy(input);
    let lines: Vec<_> = input.lines().collect();
    for mut error in diagnostics {
        error.file = source.into();
        let warning = error.level == 1;
        if !record(context, &error) || flags & if warning { 64 } else { 32 } != 0 {
            continue;
        }
        let file = if source.is_empty() { "Entity" } else { source };
        let domain = if (200..=205).contains(&error.code) {
            "namespace"
        } else {
            "parser"
        };
        let severity = if warning { "warning" } else { "error" };
        context.warning(format!(
            "{function}(): {file}:{}: {domain} {severity} : {}",
            error.line,
            error.message.trim_end()
        ));
        if let Some(line) = lines.get(error.line.saturating_sub(1)) {
            let position = error.column.saturating_sub(1);
            let start = position.saturating_sub(70);
            let snippet: String = line.chars().skip(start).take(80).collect();
            context.warning(format!("{function}(): {snippet}"));
            let spaces = " ".repeat(position.saturating_sub(start).min(80));
            context.warning(format!("{function}(): {spaces}^"));
        }
    }
}

pub(super) fn publish_schema(
    context: &mut dyn Context,
    function: &str,
    source: &str,
    errors: Vec<ParseError>,
) {
    for mut error in errors {
        error.file = source.to_owned();
        if record(context, &error) {
            context.warning(format!("{function}(): {}", error.message.trim_end()));
        }
    }
}

fn record(context: &mut dyn Context, error: &ParseError) -> bool {
    let state = context.state().get_or_default::<Errors>();
    state.last = Some(error.clone());
    if state.internal {
        state.all.push(error.clone());
        false
    } else {
        true
    }
}

pub(super) fn file_failure(context: &mut dyn Context, function: &str, path: &str) {
    let message = format!("failed to load external entity \"{path}\"");
    let error = ParseError {
        level: 1,
        code: 1549,
        column: 0,
        message: format!("{message}\n"),
        file: String::new(),
        line: 0,
    };
    let state = context.state().get_or_default::<Errors>();
    state.last = Some(error.clone());
    if state.internal {
        state.all.push(error);
    } else {
        context.warning(format!("{function}(): I/O warning : {message}"));
    }
}

pub(super) fn register(registry: &mut Registry) {
    // libxml_use_internal_errors() and its errors belong to one request.
    registry.fork_state::<Errors>(phera_extension_api::fresh_state);
    use PropertyType::{Int, String};
    use Visibility::Public;

    registry.class(ClassEntry {
        properties: const {
            &[
                PropertyEntry::typed("level", Int, Public),
                PropertyEntry::typed("code", Int, Public),
                PropertyEntry::typed("column", Int, Public),
                PropertyEntry::typed("message", String, Public),
                PropertyEntry::typed("file", String, Public),
                PropertyEntry::typed("line", Int, Public),
            ]
        },
        ..ClassEntry::class("LibXMLError")
    });
    constants::register_libxml_constants(registry);
    loaders::register(registry);
    registry.function(FunctionEntry::sync(
        "libxml_use_internal_errors",
        const {
            &[Param::new("use_errors", ParamType::Bool)
                .nullable()
                .optional()]
        },
        use_internal,
    ));
    registry.function(FunctionEntry::sync("libxml_get_last_error", &[], get_last));
    registry.function(FunctionEntry::sync("libxml_get_errors", &[], get_all));
    registry.function(FunctionEntry::sync("libxml_clear_errors", &[], clear));
}

fn use_internal(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let state = context.state().get_or_default::<Errors>();
    let previous = state.internal;
    if let Some(internal) = args::bool(arguments, 0) {
        state.internal = internal;
        if !internal {
            state.all.clear();
        }
    }
    Ok(Value::Bool(previous))
}

fn get_last(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    match context.state().get_or_default::<Errors>().last.clone() {
        Some(error) => wrap(context, error),
        None => Ok(Value::Bool(false)),
    }
}

fn get_all(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let errors = context.state().get_or_default::<Errors>().all.clone();
    let mut values = PhpArray::new();
    for error in errors {
        let _ = values.push(wrap(context, error)?);
    }
    Ok(Value::from(values))
}

fn clear(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let state = context.state().get_or_default::<Errors>();
    state.last = None;
    state.all.clear();
    Ok(Value::Null)
}

fn wrap(context: &mut dyn Context, error: ParseError) -> NativeResult {
    let mut properties = PhpArray::new();
    for (name, value) in [
        ("level", error.level.into()),
        ("code", error.code.into()),
        ("column", (error.column as i64).into()),
        ("message", error.message.into()),
        ("file", error.file.into()),
        ("line", (error.line as i64).into()),
    ] {
        properties.insert(ArrayKey::str(name), value);
    }
    context.create_object("LibXMLError", properties)
}
