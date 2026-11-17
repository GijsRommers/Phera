//! `preg_replace` and `preg_filter`.
//!
//! ```php
//! preg_replace('/(a)/', '[$1]', 'banana');         // "b[a]n[a]n[a]"
//! preg_replace(['/a/', '/b/'], ['b', 'c'], 'a');   // "c": patterns run in turn
//! preg_filter('/x/', 'y', ['ax', 'b']);            // [0 => 'ay']
//! ```
//!
//! `preg_filter` is `preg_replace` that drops the subjects in which nothing
//! was replaced.

use crate::compile::Compiled;
use crate::errors::Error;
use crate::scan::{Captures, Scanner};
use crate::{cache, errors, matches, replacement};
use phera_extension_api::value::to_php_string;
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub(crate) fn replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, Function::Replace)
}

pub(crate) fn filter(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, Function::Filter)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Function {
    Replace,
    Filter,
}

impl Function {
    fn name(self) -> &'static str {
        match self {
            Self::Replace => "preg_replace",
            Self::Filter => "preg_filter",
        }
    }
}

/// The values of an array argument, or the argument itself.
pub(crate) fn values(value: &Value) -> Vec<Value> {
    match &*value.deref() {
        Value::Array(array) => array.values().cloned().collect(),
        value => vec![value.clone()],
    }
}

/// How many matches PHP's `$limit` lets a pattern replace: a negative
/// limit (the default is -1) means all of them.
pub(crate) fn match_limit(limit: i64) -> usize {
    usize::try_from(limit).unwrap_or(usize::MAX)
}

/// The subject with some of its matches replaced, built while the matches
/// are found: the text between matches is copied as it is.
pub(crate) struct Splice<'a> {
    subject: &'a [u8],
    output: Vec<u8>,
    copied_to: usize,
}

impl<'a> Splice<'a> {
    pub(crate) fn new(subject: &'a [u8]) -> Self {
        Self {
            subject,
            output: Vec::new(),
            copied_to: 0,
        }
    }

    /// Copies the subject up to this match and skips the match. Returns the
    /// output, for the caller to append the replacement to.
    pub(crate) fn replace(&mut self, captures: &Captures) -> &mut Vec<u8> {
        let (start, end) = captures[0].expect("group 0 is the whole match");
        self.output
            .extend_from_slice(&self.subject[self.copied_to..start]);
        self.copied_to = end;
        &mut self.output
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.output
            .extend_from_slice(&self.subject[self.copied_to..]);
        self.output
    }
}

struct Operation {
    function: Function,
    patterns: Vec<Value>,
    replacements: Vec<Value>,
    /// An array of replacements pairs up with the patterns by position (a
    /// missing one is ""); a single replacement serves every pattern.
    pairwise: bool,
    limit: usize,
    /// Matches replaced so far, over all subjects: PHP's `$count`.
    count: usize,
}

fn run(context: &mut dyn Context, arguments: &[Value], function: Function) -> NativeResult {
    if args::array(arguments, 0).is_none() && args::array(arguments, 1).is_some() {
        return Err(NativeError::type_error(format!(
            "{}(): Argument #1 ($pattern) must be of type array when argument #2 \
             ($replacement) is an array, string given",
            function.name()
        )));
    }
    let mut operation = Operation {
        function,
        patterns: values(args::value(arguments, 0)),
        replacements: values(args::value(arguments, 1)),
        pairwise: args::array(arguments, 1).is_some(),
        limit: match_limit(args::int(arguments, 3).unwrap_or(-1)),
        count: 0,
    };
    let result = if let Some(subjects) = args::array(arguments, 2) {
        let mut output = PhpArray::new();
        for (key, value) in subjects.iter() {
            let result = operation.subject(context, value)?;
            if !result.is_null() {
                output.insert(key, result);
            }
        }
        Value::from(output)
    } else {
        operation.subject(context, args::value(arguments, 2))?
    };
    matches::finish(arguments, 4, operation.count, result)
}

impl Operation {
    /// Every pattern over one subject, in turn. Null when a pattern does not
    /// compile or a match fails, and for `preg_filter` when nothing matched.
    fn subject(&mut self, context: &mut dyn Context, value: &Value) -> NativeResult {
        let mut subject = to_php_string(value, context.diagnostics())?
            .as_bytes()
            .to_vec();
        let count_before = self.count;
        for (index, pattern) in self.patterns.iter().enumerate() {
            let pattern = to_php_string(pattern, context.diagnostics())?;
            let template = self
                .replacements
                .get(if self.pairwise { index } else { 0 })
                .unwrap_or(&Value::Null);
            let template = to_php_string(template, context.diagnostics())?;
            let Some(compiled) = cache::get(context, self.function.name(), pattern.as_bytes())
            else {
                return Ok(Value::Null);
            };
            errors::set(context, Error::None);
            match substitute(
                &compiled,
                &subject,
                template.as_bytes(),
                self.limit,
                &mut self.count,
            ) {
                Ok(replaced) => subject = replaced,
                Err(error) => {
                    errors::set(context, error);
                    return Ok(Value::Null);
                }
            }
        }
        if self.function == Function::Filter && self.count == count_before {
            return Ok(Value::Null);
        }
        Ok(Value::from(subject))
    }
}

/// Replaces up to `limit` matches, counting each in `count` as it goes, so
/// that the count includes the matches before a failing one.
fn substitute(
    compiled: &Compiled,
    subject: &[u8],
    template: &[u8],
    limit: usize,
    count: &mut usize,
) -> Result<Vec<u8>, Error> {
    let mut splice = Splice::new(subject);
    for found in Scanner::new(compiled, subject, 0).take(limit) {
        let captures = found?;
        replacement::expand(splice.replace(&captures), template, subject, &captures);
        *count += 1;
    }
    Ok(splice.finish())
}
