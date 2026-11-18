//! `str_replace("fox", "cat", "the fox")` is `"the cat"`.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    PhpString, Value, args,
};

use super::{CaseSensitivity, reserve};
use crate::arguments::stringify;

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
const STR_REPLACE: FunctionEntry = FunctionEntry::asynchronous(
    "str_replace",
    &[
        Param::new("search", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ],
    str_replace,
);

fn str_replace(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        replace_with(
            context,
            &arguments,
            "str_replace",
            CaseSensitivity::Sensitive,
        )
        .await
    })
}

/// `str_ireplace()`, `str_replace()` finding the search strings in any
/// ASCII case.
///
/// ```php
/// str_ireplace("FOX", "cat", "the Fox"); // "the cat"
/// ```
const STR_IREPLACE: FunctionEntry = FunctionEntry::asynchronous(
    "str_ireplace",
    &[
        Param::new("search", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ],
    str_ireplace,
);

fn str_ireplace(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        replace_with(
            context,
            &arguments,
            "str_ireplace",
            CaseSensitivity::Insensitive,
        )
        .await
    })
}

async fn replace_with(
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
    let mut replacer = Replacer {
        search,
        replace,
        case,
        count: 0,
    };
    let result = replacer.subjects(context, args::value(arguments, 2)).await;
    if let Some(count) = args::reference(arguments, 3) {
        *count.borrow_mut() = Value::Int(replacer.count);
    }
    result
}

struct Replacer<'a> {
    search: &'a Value,
    replace: &'a Value,
    case: CaseSensitivity,
    count: i64,
}

impl Replacer<'_> {
    async fn subjects(&mut self, context: &mut dyn Context, subject: &Value) -> NativeResult {
        if let Value::Array(subjects) = subject {
            let mut result = PhpArray::new();
            for (key, subject) in subjects.iter() {
                let subject = stringify(context, subject).await?;
                let value = self.replace_subject(context, subject).await?;
                result.insert(key, Value::String(value));
            }
            Ok(Value::from(result))
        } else {
            let subject = stringify(context, subject).await?;
            Ok(Value::String(self.replace_subject(context, subject).await?))
        }
    }

    /// Convert array elements for each subject independently. A replacement
    /// may have side effects even when the corresponding search does not match.
    async fn replace_subject(
        &mut self,
        context: &mut dyn Context,
        mut subject: PhpString,
    ) -> Result<PhpString, NativeError> {
        if subject.is_empty() {
            return Ok(subject);
        }
        if let Value::Array(search) = self.search {
            let mut replacements = match self.replace {
                Value::Array(array) => Some(array.values()),
                _ => None,
            };
            for term in search.values() {
                let needle = stringify(context, term).await?;
                let replacement = match &mut replacements {
                    Some(values) => values.next(),
                    None => Some(self.replace),
                };
                let replacement = match replacement {
                    Some(value) => stringify(context, value).await?,
                    None => PhpString::from(""),
                };
                if !needle.is_empty()
                    && let Some(replaced) =
                        self.replace_all(subject.as_bytes(), &needle, replacement.as_bytes())?
                {
                    subject = PhpString::from(replaced);
                }
            }
        } else {
            let needle = stringify(context, self.search).await?;
            let replacement = stringify(context, self.replace).await?;
            if !needle.is_empty()
                && let Some(replaced) =
                    self.replace_all(subject.as_bytes(), &needle, replacement.as_bytes())?
            {
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
