//! Replace operations for PHP values.

use std::rc::Rc;

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, PhpString,
    Registry, Value, args,
};

use super::{CaseSensitivity, bytes};

const PARAMS_PARAMS: &[Param] = &[
    Param::new("search", ParamType::ArrayOrString),
    Param::new("replace", ParamType::ArrayOrString),
    Param::new("subject", ParamType::ArrayOrString),
    Param::new("count", ParamType::Mixed).by_ref().optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("str_replace", PARAMS_PARAMS, replace));
    registry.function(FunctionEntry::sync("str_ireplace", PARAMS_PARAMS, ireplace));
}

fn replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    replace_subjects(context, arguments, CaseSensitivity::Sensitive)
}

fn ireplace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    replace_subjects(context, arguments, CaseSensitivity::Insensitive)
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

/// Search terms replace_subjects sequentially, so a replacement may match a later term.
fn replace_subjects(
    context: &mut dyn Context,
    arguments: &[Value],
    sensitivity: CaseSensitivity,
) -> NativeResult {
    let name = if sensitivity == CaseSensitivity::Insensitive {
        "str_ireplace"
    } else {
        "str_replace"
    };
    if args::array(arguments, 0).is_none() && args::array(arguments, 1).is_some() {
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #2 ($replace) must be of type string when argument #1 ($search) is a string"
        )));
    }
    let search = strings(context, arguments.first().unwrap_or(&Value::Null))?;
    let replacement = strings(context, arguments.get(1).unwrap_or(&Value::Null))?;
    let repeated = if args::array(arguments, 1).is_none() {
        ReplacementMode::Repeated
    } else {
        ReplacementMode::PerNeedle
    };
    let mut replacements = Replacements {
        search: &search,
        replacement: &replacement,
        repeated,
        sensitivity,
        count: 0,
    };
    let result = if let Some(array) = args::array(arguments, 2) {
        let mut result = PhpArray::new();
        for (key, value) in array.iter() {
            result.insert(key.clone(), replacements.replace_one(context, value)?);
        }
        Value::Array(Rc::new(result))
    } else {
        replacements.replace_one(context, arguments.get(2).unwrap_or(&Value::Null))?
    };
    if let Some(cell) = args::reference(arguments, 3) {
        *cell
            .try_borrow_mut()
            .map_err(|_| NativeError::error("Cannot modify borrowed reference"))? =
            Value::Int(replacements.count);
    }
    Ok(result)
}

enum ReplacementMode {
    Repeated,
    PerNeedle,
}

struct Replacements<'a> {
    search: &'a [PhpString],
    replacement: &'a [PhpString],
    repeated: ReplacementMode,
    sensitivity: CaseSensitivity,
    count: i64,
}

impl Replacements<'_> {
    /// Each search sees the output of the preceding replacement, as in PHP.
    fn replace_one(&mut self, context: &mut dyn Context, value: &Value) -> NativeResult {
        let mut string = to_php_string(value, context.diagnostics())?
            .as_bytes()
            .to_vec();
        for (i, needle) in self.search.iter().enumerate() {
            if needle.is_empty() {
                continue;
            }
            let replacement_bytes = self
                .replacement
                .get(if matches!(self.repeated, ReplacementMode::Repeated) {
                    0
                } else {
                    i
                })
                .map_or(&[][..], PhpString::as_bytes);
            let mut result = Vec::new();
            let mut start = 0;
            while let Some(tail) = string.get(start..) {
                let Some(position) = (if self.sensitivity == CaseSensitivity::Insensitive {
                    super::find_ignore_case(tail, needle.as_bytes())
                } else {
                    super::find(tail, needle.as_bytes())
                }) else {
                    break;
                };
                super::reserve(
                    &mut result,
                    position.saturating_add(replacement_bytes.len()),
                )?;
                result.extend_from_slice(tail.get(..position).unwrap_or_default());
                result.extend_from_slice(replacement_bytes);
                start += position + needle.len();
                self.count = self.count.saturating_add(1);
            }
            let tail = string.get(start..).unwrap_or_default();
            super::reserve(&mut result, tail.len())?;
            result.extend_from_slice(tail);
            string = result;
        }
        Ok(bytes(string))
    }
}
