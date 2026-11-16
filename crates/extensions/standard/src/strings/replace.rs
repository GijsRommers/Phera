//! Replace operations for PHP values.

use std::rc::Rc;

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, PhpString,
    Registry, Value, args,
};

use super::bytes;

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("search", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ];
    registry.function(FunctionEntry::sync("str_replace", PARAMS, replace));
    registry.function(FunctionEntry::sync("str_ireplace", PARAMS, ireplace));
}

fn replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, false)
}

fn ireplace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, true)
}

/// Array replacements convert each element using PHP string coercion and diagnostics.
fn strings(context: &mut dyn Context, value: &Value) -> Result<Vec<PhpString>, NativeError> {
    match value {
        Value::Array(array) => array
            .values()
            .map(|value| to_php_string(value, context.diagnostics()).map_err(Into::into))
            .collect(),
        _ => Ok(vec![to_php_string(value, context.diagnostics())?]),
    }
}

/// Search terms run sequentially, so a replacement may match a later term.
fn run(context: &mut dyn Context, arguments: &[Value], fold: bool) -> NativeResult {
    let name = if fold { "str_ireplace" } else { "str_replace" };
    if args::array(arguments, 0).is_none() && args::array(arguments, 1).is_some() {
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #2 ($replace) must be of type string when argument #1 ($search) is a string"
        )));
    }
    let search = strings(context, arguments.first().unwrap_or(&Value::Null))?;
    let replacement = strings(context, arguments.get(1).unwrap_or(&Value::Null))?;
    let repeated = args::array(arguments, 1).is_none();
    let mut count = 0i64;
    let mut replace_one = |context: &mut dyn Context, value: &Value| -> NativeResult {
        let mut string = to_php_string(value, context.diagnostics())?
            .as_bytes()
            .to_vec();
        for (i, n) in search.iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            let rep = replacement
                .get(if repeated { 0 } else { i })
                .map_or(&[][..], PhpString::as_bytes);
            let mut result = Vec::new();
            let mut start = 0;
            while let Some(tail) = string.get(start..) {
                let Some(pos) = (if fold {
                    super::find_ignore_case(tail, n.as_bytes())
                } else {
                    super::find(tail, n.as_bytes())
                }) else {
                    break;
                };
                super::reserve(&mut result, pos.saturating_add(rep.len()))?;
                result.extend_from_slice(tail.get(..pos).unwrap_or_default());
                result.extend_from_slice(rep);
                start += pos + n.len();
                count = count.saturating_add(1);
            }
            let tail = string.get(start..).unwrap_or_default();
            super::reserve(&mut result, tail.len())?;
            result.extend_from_slice(tail);
            string = result;
        }
        Ok(bytes(string))
    };
    let result = if let Some(array) = args::array(arguments, 2) {
        let mut result = PhpArray::new();
        for (key, value) in array.iter() {
            result.insert(key.clone(), replace_one(context, value)?);
        }
        Value::Array(Rc::new(result))
    } else {
        replace_one(context, arguments.get(2).unwrap_or(&Value::Null))?
    };
    if let Some(cell) = args::reference(arguments, 3) {
        *cell
            .try_borrow_mut()
            .map_err(|_| NativeError::error("Cannot modify borrowed reference"))? =
            Value::Int(count);
    }
    Ok(result)
}
