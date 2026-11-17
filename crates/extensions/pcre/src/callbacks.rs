//! Callbacks await PHP: `preg_replace_callback('/a/', fn($m) => strtoupper($m[0]), 'a')`.

use crate::errors::Error;
use crate::flags::CaptureFlags;
use crate::scan::Scanner;
use crate::{cache, callable, errors, matches, replace};
use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args,
};

struct Operation<'a> {
    name: &'static str,
    patterns: Vec<Value>,
    callback: &'a Value,
    limit: i64,
    flags: CaptureFlags,
    count: usize,
    exception: Option<NativeError>,
}
impl Operation<'_> {
    async fn subject(&mut self, context: &mut dyn Context, value: &Value) -> NativeResult {
        let mut subject = to_php_string(value, context.diagnostics())?
            .as_bytes()
            .to_vec();
        for pattern in &self.patterns {
            let pattern = to_php_string(pattern, context.diagnostics())?;
            let compiled = if self.exception.is_some() {
                // PHP suppresses new warnings while propagating an existing exception.
                match cache::lookup(pattern.as_bytes()) {
                    Ok(compiled) => Some(compiled),
                    Err(_) => {
                        errors::set(context, Error::Internal);
                        None
                    }
                }
            } else {
                cache::get(context, self.name, pattern.as_bytes())
            };
            let Some(compiled) = compiled else {
                return Ok(Value::Null);
            };
            errors::set(context, Error::None);
            let mut scanner = Scanner::new(&compiled, &subject, 0);
            let mut output = Vec::new();
            let mut previous_end = 0;
            let mut replaced = 0;
            while self.limit < 0 || replaced < self.limit {
                let captures = match scanner.next() {
                    Ok(Some(captures)) => captures,
                    Ok(None) => break,
                    Err(error) => {
                        errors::set(context, error);
                        return Ok(Value::Null);
                    }
                };
                let (start, end) = captures[0].expect("whole match");
                let parameters = matches::row(
                    &subject,
                    &captures,
                    compiled.regex.capture_names(),
                    self.flags,
                );
                self.count += 1;
                // PHP still scans subsequent array subjects while an exception is pending,
                // but never invokes their callbacks. Keep the original unwind untouched.
                if self.exception.is_some() {
                    errors::set(context, Error::Internal);
                    return Ok(Value::Null);
                }
                let replacement = match context.call(self.callback, vec![parameters]).await {
                    Ok(value) => value,
                    Err(error) => {
                        errors::set(context, Error::Internal);
                        self.exception = Some(error);
                        return Ok(Value::Null);
                    }
                };
                let replacement = to_php_string(&replacement, context.diagnostics())?;
                output.extend_from_slice(&subject[previous_end..start]);
                output.extend_from_slice(replacement.as_bytes());
                previous_end = end;
                replaced += 1;
            }
            output.extend_from_slice(&subject[previous_end..]);
            subject = output;
        }
        Ok(Value::from(subject))
    }
    async fn apply(&mut self, context: &mut dyn Context, subject: &Value) -> NativeResult {
        if let Value::Array(subjects) = &*subject.deref() {
            let mut output = PhpArray::new();
            for (key, value) in subjects.iter() {
                let result = self.subject(context, value).await?;
                if !result.is_null() {
                    output.insert(key, result);
                }
            }
            Ok(Value::from(output))
        } else {
            self.subject(context, subject).await
        }
    }
}

pub(crate) fn replace(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        callable::validate(args::value(&arguments, 1), false)?;
        let mut operation = Operation {
            name: "preg_replace_callback",
            patterns: replace::values(args::value(&arguments, 0)),
            callback: args::value(&arguments, 1),
            limit: args::int(&arguments, 3).unwrap_or(-1),
            flags: CaptureFlags::parse(args::int(&arguments, 5).unwrap_or(0)),
            count: 0,
            exception: None,
        };
        let result = operation.apply(context, args::value(&arguments, 2)).await;
        matches::assign(&arguments, 4, Value::Int(operation.count as i64));
        if let Some(error) = operation.exception {
            return Err(error);
        }
        result
    })
}
pub(crate) fn replace_array(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut subject = args::value(&arguments, 1).clone();
        let mut count = 0;
        if let Some(patterns) = args::array(&arguments, 0) {
            for (key, callback) in patterns.iter() {
                let ArrayKey::String(pattern) = key else {
                    return Err(NativeError::type_error(
                        "preg_replace_callback_array(): Argument #1 ($pattern) \
                         must contain only string patterns as keys",
                    ));
                };
                callable::validate(callback, true)?;
                let mut operation = Operation {
                    name: "preg_replace_callback_array",
                    patterns: vec![Value::String(pattern.clone())],
                    callback,
                    limit: args::int(&arguments, 2).unwrap_or(-1),
                    flags: CaptureFlags::parse(args::int(&arguments, 4).unwrap_or(0)),
                    count: 0,
                    exception: None,
                };
                subject = operation.apply(context, &subject).await?;
                if let Some(error) = operation.exception {
                    return Err(error);
                }
                if subject.is_null() {
                    return Ok(Value::Null);
                }
                count += operation.count;
            }
        }
        matches::finish(&arguments, 3, count, subject)
    })
}
