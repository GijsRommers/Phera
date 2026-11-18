//! Native entry points for a single value, a uniform array, or a keyed definition.
//!
//! ```php
//! filter_var_array(['id' => '3'], ['id' => FILTER_VALIDATE_INT, 'name' => FILTER_DEFAULT]);
//! // ['id' => 3, 'name' => null]
//! ```

use phera_extension_api::value::to_int;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args, prepare_call,
};

use crate::apply;
use crate::constants::{CALLBACK, DEFAULT, REQUIRE_ARRAY, REQUIRE_SCALAR, SANITIZE_ADD_SLASHES};
use crate::options::{Call, Options};

const VAR: &[Param] = &[
    Param::new("value", ParamType::Mixed),
    Param::new("filter", ParamType::Int).optional(),
    Param::new("options", ParamType::Mixed).optional(),
];
const ARRAY: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("options", ParamType::Mixed).optional(),
    Param::new("add_empty", ParamType::Mixed).optional(),
];

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous("filter_var", VAR, var));
    registry.function(FunctionEntry::asynchronous(
        "filter_var_array",
        ARRAY,
        array,
    ));
}

fn var(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let id = args::int(&arguments, 1).unwrap_or(DEFAULT);
        let definition = if let Some(value) = arguments.get(2) {
            Call::Var.union(context, value)?
        } else {
            Value::Int(0)
        };

        if !known(id) {
            warn(context, Call::Var, id);
            return Ok(Value::Bool(false));
        }
        let options = Options::new(context, Call::Var, id, REQUIRE_SCALAR, &definition)?;
        apply::value(
            context,
            arguments.into_iter().next().unwrap_or_default(),
            &options,
        )
        .await
    })
}

fn array(context: &mut dyn Context, mut arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let definition = if let Some(value) = arguments.get(1) {
            Call::Array.union(context, value)?
        } else {
            Value::Int(DEFAULT)
        };
        // PHP checks the union before coercing add_empty.
        const PARSED: &[Param] = &[
            Param::new("array", ParamType::Array),
            Param::new("options", ParamType::Mixed).optional(),
            Param::new("add_empty", ParamType::Bool).optional(),
        ];
        prepare_call(Call::Array.name(), PARSED, &mut arguments, context)?;
        let add_empty = args::bool(&arguments, 2).unwrap_or(true);
        let Value::Array(input) = arguments.remove(0) else {
            unreachable!("signature requires an array")
        };
        match definition {
            Value::Array(definition) => by_key(context, &input, &definition, add_empty).await,
            Value::Int(id) if !known(id) => {
                warn(context, Call::Array, id);
                Ok(Value::Bool(false))
            }
            _ => {
                let options = Options::new(context, Call::Array, -1, REQUIRE_ARRAY, &definition)?;
                apply::value(context, Value::Array(input), &options).await
            }
        }
    })
}

async fn by_key(
    context: &mut dyn Context,
    input: &PhpArray,
    definition: &PhpArray,
    add_empty: bool,
) -> NativeResult {
    let mut result = PhpArray::new();
    for (key, filter) in definition.iter() {
        let ArrayKey::String(name) = &key else {
            return Err(NativeError::type_error(
                "filter_var_array(): Argument #2 ($options) must contain only string keys",
            ));
        };
        if name.is_empty() {
            return Err(Call::Array.option_error("cannot contain empty keys"));
        }
        if let Some(source) = input.get(&key) {
            let filter = if filter.is_array() && !filter.is_reference() {
                filter.clone()
            } else {
                let id = to_int(filter, context.diagnostics())?;
                if !known(id) {
                    warn(context, Call::Array, id);
                }
                // PHP converts a non-array entry a second time when applying it.
                Value::Int(to_int(filter, context.diagnostics())?)
            };
            let options = Options::new(context, Call::Array, -1, REQUIRE_SCALAR, &filter)?;
            let source = source.deref().clone();
            let filtered = apply::value(context, source, &options).await?;
            result.insert(key, filtered);
        } else if add_empty {
            result.insert(key, Value::Null);
        }
    }
    Ok(Value::from(result))
}

/// PHP accepts gaps in the numeric filter ID ranges and treats them as raw.
fn known(id: i64) -> bool {
    matches!(id, 256..=277 | 512..=SANITIZE_ADD_SLASHES | CALLBACK)
}

fn warn(context: &mut dyn Context, call: Call, id: i64) {
    context.warning(format!("{}(): Unknown filter with ID {id}", call.name()));
}
