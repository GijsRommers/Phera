//! `str_replace("fox", "cat", "the fox")` is `"the cat"`.

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, PhpString,
    Value, args,
};

use super::{CaseSensitivity, reserve};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[STR_REPLACE, STR_IREPLACE];

/// `str_replace(array|string $search, array|string $replace,
/// string|array $subject, &$count = null): string|array`
///
/// With arrays, each search string is replaced in turn, in the result of
/// the one before, so a replacement can be replaced again by a later
/// search. A missing replacement is the empty string. An array subject is
/// replaced in value by value, keeping its keys.
///
/// ```php
/// str_replace("fox", "cat", "the fox");      // "the cat"
/// str_replace(["a", "b"], ["b", "c"], "ab"); // "cc"
/// str_replace("a", "x", ["k" => "aa"], $n);  // ["k" => "xx"], and $n is 2
/// ```
const STR_REPLACE: FunctionEntry = FunctionEntry::sync(
    "str_replace",
    &[
        Param::new("search", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ],
    str_replace,
);

fn str_replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    replace_with(
        context,
        arguments,
        "str_replace",
        CaseSensitivity::Sensitive,
    )
}

/// `str_ireplace()`, `str_replace()` finding the search strings in any
/// ASCII case.
///
/// ```php
/// str_ireplace("FOX", "cat", "the Fox"); // "the cat"
/// ```
const STR_IREPLACE: FunctionEntry = FunctionEntry::sync(
    "str_ireplace",
    &[
        Param::new("search", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ],
    str_ireplace,
);

fn str_ireplace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    replace_with(
        context,
        arguments,
        "str_ireplace",
        CaseSensitivity::Insensitive,
    )
}

fn replace_with(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    case: CaseSensitivity,
) -> NativeResult {
    let search = args::value(arguments, 0);
    let replace = args::value(arguments, 1);
    if !search.is_array() && replace.is_array() {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #2 ($replace) must be of type string \
             when argument #1 ($search) is a string"
        )));
    }
    let search = strings(context, search)?;
    let replacements = if replace.is_array() {
        Replacements::OnePerSearch(strings(context, replace)?)
    } else {
        Replacements::Same(to_php_string(replace, context.diagnostics())?)
    };
    let mut replacer = Replacer {
        search: &search,
        replacements: &replacements,
        case,
        count: 0,
    };
    let result = if let Some(subjects) = args::array(arguments, 2) {
        let mut result = PhpArray::new();
        for (key, subject) in subjects.iter() {
            let subject = to_php_string(subject, context.diagnostics())?;
            result.insert(key, Value::String(replacer.replace(subject)?));
        }
        Value::from(result)
    } else {
        let subject = to_php_string(args::value(arguments, 2), context.diagnostics())?;
        Value::String(replacer.replace(subject)?)
    };
    if let Some(count) = args::reference(arguments, 3) {
        *count.borrow_mut() = Value::Int(replacer.count);
    }
    Ok(result)
}

/// A string, or the values of an array as strings, in order.
fn strings(context: &mut dyn Context, value: &Value) -> Result<Vec<PhpString>, NativeError> {
    match value {
        Value::Array(array) => array
            .values()
            .map(|value| Ok(to_php_string(value, context.diagnostics())?))
            .collect(),
        _ => Ok(vec![to_php_string(value, context.diagnostics())?]),
    }
}

enum Replacements {
    /// `$replace` was a string, which replaces every search string.
    Same(PhpString),
    /// `$replace` was an array: the n-th replaces the n-th search string.
    OnePerSearch(Vec<PhpString>),
}

impl Replacements {
    fn for_search(&self, index: usize) -> &[u8] {
        match self {
            Self::Same(replacement) => replacement.as_bytes(),
            Self::OnePerSearch(list) => list.get(index).map_or(&[], PhpString::as_bytes),
        }
    }
}

struct Replacer<'a> {
    search: &'a [PhpString],
    replacements: &'a Replacements,
    case: CaseSensitivity,
    /// The replacements made so far, for `$count`.
    count: i64,
}

impl Replacer<'_> {
    fn replace(&mut self, mut subject: PhpString) -> Result<PhpString, NativeError> {
        for (index, needle) in self.search.iter().enumerate() {
            // PHP skips empty search strings instead of matching everywhere.
            if needle.is_empty() {
                continue;
            }
            let replacement = self.replacements.for_search(index);
            if let Some(replaced) = self.replace_all(subject.as_bytes(), needle, replacement)? {
                subject = PhpString::from(replaced);
            }
        }
        Ok(subject)
    }

    /// The subject with every `needle` replaced, or `None` when there is
    /// none, so an unchanged subject keeps sharing its bytes.
    fn replace_all(
        &mut self,
        subject: &[u8],
        needle: &[u8],
        replacement: &[u8],
    ) -> Result<Option<Vec<u8>>, NativeError> {
        let mut result = Vec::new();
        let mut rest = subject;
        while let Some(position) = self.case.find(rest, needle) {
            reserve(&mut result, position + replacement.len())?;
            result.extend_from_slice(&rest[..position]);
            result.extend_from_slice(replacement);
            rest = &rest[position + needle.len()..];
            self.count += 1;
        }
        if rest.len() == subject.len() {
            return Ok(None);
        }
        reserve(&mut result, rest.len())?;
        result.extend_from_slice(rest);
        Ok(Some(result))
    }
}
