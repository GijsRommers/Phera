//! `$stmt->bind_param('is', $id, $name)` reads references on each execute.

use mysql_async::Value as Parameter;
use phera_extension_api::{
    Context, NativeError, NativeResult, PhpArray, Value, args,
    value::{to_float, to_int, to_php_string},
};

use super::{Reference, State};

#[derive(Clone, Copy)]
pub enum Kind {
    Integer,
    Double,
    String,
    Blob,
}

pub fn bind(state: &mut State, arguments: &[Value], call: &str, arg: usize) -> NativeResult {
    let types = args::bytes(arguments, 0);
    let refs = references(arguments.get(1..).unwrap_or_default());
    if types.is_empty() {
        return Err(NativeError::value_error(format!(
            "{call}(): Argument #{arg} ($types) must not be empty"
        )));
    }
    if types.len() != refs.len() {
        return Err(NativeError::argument_count_error(
            "The number of elements in the type definition string must match the number of bind variables",
        ));
    }
    if types.len() != usize::from(state.statement()?.num_params()) {
        return Err(NativeError::argument_count_error(
            "The number of variables must match the number of parameters in the prepared statement",
        ));
    }
    let kinds: Result<Vec<Kind>, NativeError> = types.iter().map(|kind| match kind {
        b'i' => Ok(Kind::Integer),
        b'd' => Ok(Kind::Double),
        b's' => Ok(Kind::String),
        b'b' => Ok(Kind::Blob),
        _ => Err(NativeError::value_error(format!(
            "{call}(): Argument #{arg} ($types) must only contain the \"b\", \"d\", \"i\", \"s\"              type specifiers"
        ))),
    }).collect();
    state.parameters = kinds?.into_iter().zip(refs).collect();
    Ok(Value::Bool(true))
}

pub fn bind_result(state: &mut State, arguments: &[Value]) -> NativeResult {
    let refs = references(arguments);
    if refs.len() != state.statement()?.columns().len() {
        return Err(NativeError::argument_count_error(
            "Number of bind variables doesn't match number of fields in prepared statement",
        ));
    }
    state.output = refs;
    Ok(Value::Bool(true))
}

fn references(arguments: &[Value]) -> Vec<Reference> {
    arguments
        .iter()
        .enumerate()
        .filter_map(|(i, _)| args::reference(arguments, i).cloned())
        .collect()
}

pub fn parameters(
    context: &mut dyn Context,
    state: &mut State,
    list: Option<&PhpArray>,
    call: &str,
) -> Result<Vec<Parameter>, NativeError> {
    let count = usize::from(state.statement()?.num_params());
    if let Some(list) = list {
        let arg = if call.ends_with("execute_query") {
            if call.contains("::") { 2 } else { 3 }
        } else if call.contains("::") {
            1
        } else {
            2
        };
        if !list.is_list() {
            return Err(NativeError::value_error(format!(
                "{call}(): Argument #{arg} ($params) must be a list array"
            )));
        }
        if list.len() != count {
            return Err(NativeError::value_error(format!(
                "{call}(): Argument #{arg} ($params) must consist of exactly {count} elements, \
                 {} present",
                list.len(),
            )));
        }
        return list
            .iter()
            .map(|(_, value)| parameter(context, value, Kind::String))
            .collect();
    }
    let long = std::mem::take(&mut state.long_data);
    state
        .parameters
        .iter()
        .enumerate()
        .map(|(i, (kind, reference))| {
            if let Some(bytes) = long.get(&i) {
                return Ok(Parameter::Bytes(bytes.clone()));
            }
            parameter(context, &reference.borrow(), *kind)
        })
        .collect()
}

fn parameter(
    context: &mut dyn Context,
    value: &Value,
    kind: Kind,
) -> Result<Parameter, NativeError> {
    if matches!(&*value.deref(), Value::Null) {
        return Ok(Parameter::NULL);
    }
    Ok(match kind {
        Kind::Integer => Parameter::Int(to_int(value, context.diagnostics())?),
        Kind::Double => Parameter::Double(to_float(value, context.diagnostics())?),
        Kind::String | Kind::Blob => Parameter::Bytes(
            to_php_string(value, context.diagnostics())?
                .as_bytes()
                .to_vec(),
        ),
    })
}
