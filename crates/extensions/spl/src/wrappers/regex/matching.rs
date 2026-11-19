//! Regular expression actions reuse PHP's preg matching, capture and replacement rules.
//!
//! ```php
//! $it->setMode(RegexIterator::REPLACE);
//! $it->replacement = '$1';
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value,
};

use super::{Mode, RECURSIVE, get};
use crate::{
    receiver,
    wrappers::{cache::stringify, inner, read, update},
};

pub(super) fn accept(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (value, key) = read(object, |state| (state.value.clone(), state.key.clone()))?;
        let Some(value) = value else {
            return Ok(Value::Bool(false));
        };
        let regex = get(object, Clone::clone)?;
        let use_key = regex.flags & 1 != 0;
        if let Value::Array(array) = &*value.deref() {
            if object.class.is_instance_of(RECURSIVE) {
                return Ok(Value::Bool(!array.is_empty()));
            }
            if !use_key {
                return Ok(Value::Bool(false));
            }
        }
        let subject = stringify(context, if use_key { &key } else { &value }).await?;
        let matched = match regex.mode {
            Mode::Match => matches!(&subject, Value::String(subject)
                if phera_ext_pcre::simple_match(regex.pattern.as_bytes(), subject.as_bytes())),
            Mode::GetMatch | Mode::AllMatches => {
                let captures = Rc::new(RefCell::new(Value::Null));
                let name = if matches!(regex.mode, Mode::GetMatch) {
                    "preg_match"
                } else {
                    "preg_match_all"
                };
                let count = context
                    .call(
                        &Value::from(name),
                        vec![
                            Value::String(regex.pattern),
                            subject,
                            Value::Reference(captures.clone()),
                            Value::Int(regex.preg_flags),
                        ],
                    )
                    .await?;
                let data = captures.try_borrow().map_err(|_| crate::busy())?.clone();
                update(object, |state| state.value = Some(data))?;
                matches!(count, Value::Int(count) if count > 0)
            }
            Mode::Split => {
                let pieces = context
                    .call(
                        &Value::from("preg_split"),
                        vec![
                            Value::String(regex.pattern),
                            subject,
                            Value::Int(-1),
                            Value::Int(regex.preg_flags),
                        ],
                    )
                    .await?;
                let matched = matches!(&pieces, Value::Array(array) if array.len() > 1);
                update(object, |state| state.value = Some(pieces))?;
                matched
            }
            Mode::Replace => {
                let count = Rc::new(RefCell::new(Value::Null));
                let replacement = object
                    .get(&ArrayKey::str("replacement"))
                    .unwrap_or(Value::Null);
                let replacement = stringify(context, &replacement).await?;
                let replaced = context
                    .call(
                        &Value::from("preg_replace"),
                        vec![
                            Value::String(regex.pattern),
                            replacement,
                            subject,
                            Value::Int(-1),
                            Value::Reference(count.clone()),
                        ],
                    )
                    .await?;
                if replaced.is_null() {
                    return Ok(Value::Bool(false));
                }
                update(object, |state| {
                    if use_key {
                        state.key = replaced;
                    } else {
                        state.value = Some(replaced);
                    }
                })?;
                matches!(&*count.try_borrow().map_err(|_| crate::busy())?, Value::Int(count) if *count > 0)
            }
        };
        Ok(Value::Bool(matched ^ (regex.flags & 2 != 0)))
    })
}

pub(super) fn children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let child = context
            .call_method(inner(object)?, "getChildren", Vec::new())
            .await?;
        let regex = get(object, Clone::clone)?;
        let next = context.create_object(object.class.name(), PhpArray::new())?;
        let Value::Object(next_object) = &next else {
            return Err(NativeError::error("Cannot create recursive regex iterator"));
        };
        let arguments = vec![
            child,
            Value::String(regex.pattern),
            Value::Int(regex.mode.raw()),
            Value::Int(regex.flags),
            Value::Int(regex.preg_flags),
        ];
        context
            .call_method(next_object.clone(), "__construct", arguments)
            .await?;
        Ok(next)
    })
}
