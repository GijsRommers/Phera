//! `json_encode`.
//!
//! ```php
//! json_encode(['a' => 1, 'b' => [true, null]]);   // {"a":1,"b":[true,null]}
//! json_encode([1 => 'x']);                         // {"1":"x"}: not a list
//! json_encode(NAN);                                // false, JSON_ERROR_INF_OR_NAN
//! json_encode(NAN, JSON_PARTIAL_OUTPUT_ON_ERROR);  // "0"
//! $a = []; $a[] = &$a; json_encode($a);            // false, JSON_ERROR_RECURSION
//! ```
//!
//! The encoder keeps a stack of work instead of recursing, so that deeply
//! nested values cannot overflow the Rust stack. A container being encoded
//! is "active"; meeting it again inside itself is recursion, meeting it
//! again elsewhere (the same array twice in a list) is not.

use crate::error::{self, Error};
use crate::flags::EncodeFlags;
use crate::recursion::Serializing;
use crate::string::encode_string;
use phera_extension_api::value::{
    NumVal, PropertyScope, float_repr, parse_numeric, split_property_key,
};
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PropertyPurpose,
    Value, args,
};
use std::rc::Rc;
use std::{cell::RefCell, collections::HashSet};

/// What is left to write, last item first.
enum Work {
    /// A value at nesting `level`.
    Value {
        value: Value,
        level: usize,
    },
    /// Punctuation: `,`, `:` or `: `.
    Text(&'static str),
    /// A newline and the indentation of `level`, for `JSON_PRETTY_PRINT`.
    Indent(usize),
    /// An object key, written as a JSON string.
    Key(ArrayKey),
    /// Keep the container alive until its address leaves the recursion set.
    Close {
        owner: Owner,
        level: usize,
        closing: char,
        empty: bool,
    },
    /// The end of the value behind a reference.
    Release(Rc<RefCell<Value>>),
    ReleaseSerializer(Serializing),
}

enum Owner {
    Array(Rc<PhpArray>),
    Object(Rc<Object>),
}

impl Owner {
    fn identity(&self) -> usize {
        match self {
            Self::Array(array) => Rc::as_ptr(array) as usize,
            Self::Object(object) => Rc::as_ptr(object) as usize,
        }
    }
}

struct Encoder {
    flags: EncodeFlags,
    depth: i64,
    output: String,
    error: Error,
    /// The containers and references being encoded, by address.
    active: HashSet<usize>,
    work: Vec<Work>,
}

/// PHP checks `$depth` when a container is finished, so a too deep value
/// is still written in full with `JSON_PARTIAL_OUTPUT_ON_ERROR`.
pub(crate) fn json_encode(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(encode(context, arguments))
}

async fn encode(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let flags = EncodeFlags::from_bits(args::int(&arguments, 1).unwrap_or(0));
    let value = arguments.first().cloned().unwrap_or_default();
    let mut encoder = Encoder {
        flags,
        // PHP keeps the depth in a C int.
        depth: i64::from(args::int(&arguments, 2).unwrap_or(512) as i32),
        output: String::new(),
        error: Error::None,
        active: HashSet::new(),
        work: vec![Work::Value { value, level: 0 }],
    };
    if let Err(exception) = encoder.run(context).await {
        if !flags.throw_on_error || flags.partial_output {
            error::finish(context, encoder.error, false, Value::Null)?;
        }
        return Err(exception);
    }
    let result = if encoder.error == Error::None || flags.partial_output {
        Value::from(encoder.output)
    } else {
        Value::Bool(false)
    };
    error::finish(
        context,
        encoder.error,
        flags.throw_on_error && !flags.partial_output,
        result,
    )
}

impl Encoder {
    /// Works until done, or until the first error unless the output is
    /// partial: then it goes on, and the last error is the one reported.
    async fn run(&mut self, context: &mut dyn Context) -> Result<(), NativeError> {
        while let Some(work) = self.work.pop() {
            match work {
                Work::Value {
                    value: Value::Object(object),
                    level,
                } => {
                    self.object(context, object, level).await?;
                }
                Work::Value { value, level } => self.value(value, level),
                Work::ReleaseSerializer(guard) => drop(guard),
                Work::Text(text) => self.output.push_str(text),
                Work::Indent(level) => self.indent(level),
                Work::Key(key) => self.key(key),
                Work::Release(reference) => {
                    self.active.remove(&(Rc::as_ptr(&reference) as usize));
                }
                Work::Close {
                    owner,
                    level,
                    closing,
                    empty,
                } => {
                    self.active.remove(&owner.identity());
                    if level as i64 > self.depth {
                        self.error = Error::Depth;
                    }
                    if !empty {
                        self.indent(level - 1);
                    }
                    self.output.push(closing);
                }
            }
            if self.error != Error::None && !self.flags.partial_output {
                break;
            }
        }
        Ok(())
    }

    /// Records an error and writes what PHP writes in place of the value:
    /// 0 for a float that is not finite, null for anything else.
    fn failure(&mut self, error: Error) {
        self.error = error;
        self.output
            .push_str(if matches!(error, Error::Infinite | Error::NonBackedEnum) {
                "0"
            } else {
                "null"
            });
    }

    fn indent(&mut self, level: usize) {
        if self.flags.pretty_print {
            self.output.push('\n');
            for _ in 0..level {
                self.output.push_str("    ");
            }
        }
    }

    /// A key that is not UTF-8 is written as `""`, not null.
    fn key(&mut self, key: ArrayKey) {
        match key {
            ArrayKey::Int(integer) => {
                self.output.push('"');
                self.output.push_str(&integer.to_string());
                self.output.push('"');
            }
            ArrayKey::String(name) => {
                let start = self.output.len();
                if let Err(error) = encode_string(&mut self.output, name.as_bytes(), self.flags) {
                    self.output.truncate(start);
                    self.error = error;
                    self.output.push_str("\"\"");
                }
            }
        }
    }

    fn value(&mut self, value: Value, level: usize) {
        match value {
            Value::Null => self.output.push_str("null"),
            Value::Bool(boolean) => self.output.push_str(if boolean { "true" } else { "false" }),
            Value::Int(integer) => self.output.push_str(&integer.to_string()),
            Value::Float(number) => self.number(number),
            Value::String(bytes) => self.string(bytes.as_bytes()),
            Value::Resource(_) => self.failure(Error::Unsupported),
            Value::Reference(reference) => {
                let identity = Rc::as_ptr(&reference) as usize;
                if !self.active.insert(identity) {
                    self.failure(Error::Recursion);
                    return;
                }
                // A reference already borrowed for writing is one being
                // changed further up: recursion too.
                if let Ok(value) = reference.try_borrow() {
                    self.work.push(Work::Release(reference.clone()));
                    self.work.push(Work::Value {
                        value: value.clone(),
                        level,
                    });
                } else {
                    self.failure(Error::Recursion);
                }
            }
            Value::Array(array) => {
                let entries = array
                    .iter()
                    .map(|(key, value)| (key, value.clone()))
                    .collect();
                let object = !array.is_list() || self.flags.force_object;
                self.container(Owner::Array(array), entries, object, level);
            }
            Value::Object(_) => {}
        }
    }

    async fn object(
        &mut self,
        context: &mut dyn Context,
        object: Rc<Object>,
        level: usize,
    ) -> Result<(), NativeError> {
        if object.class.is_instance_of("JsonSerializable") {
            let Some(guard) = Serializing::start(context, object.clone()) else {
                self.failure(Error::Recursion);
                return Ok(());
            };
            let value = context
                .call_protocol_method(object.clone(), "jsonSerialize", Vec::new())
                .await?;
            if !matches!(&*value.deref(), Value::Object(result) if Rc::ptr_eq(&object, result)) {
                self.work.push(Work::ReleaseSerializer(guard));
                self.work.push(Work::Value { value, level });
                return Ok(());
            }
        } else if object.class.is_enum() {
            if let Some(value) = object.get(&ArrayKey::str("value")) {
                self.work.push(Work::Value { value, level });
            } else {
                self.failure(Error::NonBackedEnum);
            }
            return Ok(());
        }
        let identity = Rc::as_ptr(&object) as usize;
        if self.active.contains(&identity) {
            self.failure(Error::Recursion);
            return Ok(());
        }
        let properties = if context.property_view_runs_code(&object, PropertyPurpose::Json) {
            match context
                .property_view(object.clone(), PropertyPurpose::Json, "json_encode", &[])
                .await?
            {
                phera_extension_api::PropertyView::Snapshot(properties)
                | phera_extension_api::PropertyView::DebugInfo(properties) => properties,
                phera_extension_api::PropertyView::DebugSlots { slots, .. } => slots,
                phera_extension_api::PropertyView::Recursive => {
                    self.failure(Error::Recursion);
                    return Ok(());
                }
            }
        } else {
            Rc::new(context.object_properties(&object, PropertyPurpose::Json)?)
        };
        let entries = properties
            .iter()
            .filter(|(key, _)| match key {
                ArrayKey::String(name) => {
                    split_property_key(name.as_bytes()).1 == PropertyScope::Public
                }
                ArrayKey::Int(_) => true,
            })
            .map(|(key, value)| (key, value.clone()))
            .collect();
        self.container(Owner::Object(object), entries, true, level);
        Ok(())
    }

    /// Writes the opening bracket and schedules the entries and the closing
    /// bracket. The work stack runs last item first, so the items are
    /// pushed from the last entry back to the first.
    fn container(
        &mut self,
        owner: Owner,
        entries: Vec<(ArrayKey, Value)>,
        object: bool,
        level: usize,
    ) {
        if !self.active.insert(owner.identity()) {
            self.failure(Error::Recursion);
            return;
        }
        self.output.push(if object { '{' } else { '[' });
        self.work.push(Work::Close {
            owner,
            level: level + 1,
            closing: if object { '}' } else { ']' },
            empty: entries.is_empty(),
        });
        for (index, (key, value)) in entries.into_iter().enumerate().rev() {
            self.work.push(Work::Value {
                value,
                level: level + 1,
            });
            if object {
                self.work
                    .push(Work::Text(if self.flags.pretty_print { ": " } else { ":" }));
                self.work.push(Work::Key(key));
            }
            self.work.push(Work::Indent(level + 1));
            if index != 0 {
                self.work.push(Work::Text(","));
            }
        }
    }

    /// A float in the shortest decimal form that reads back as the same
    /// float (PHP's `serialize_precision` of -1), with a lower-case `e`.
    fn number(&mut self, number: f64) {
        if !number.is_finite() {
            self.failure(Error::Infinite);
            return;
        }
        let text = float_repr(number).replace('E', "e");
        self.output.push_str(&text);
        if self.flags.preserve_zero_fraction && !text.contains(['.', 'e']) {
            self.output.push_str(".0");
        }
    }

    /// With `JSON_NUMERIC_CHECK` a numeric string is written as a number.
    /// Keys are never checked.
    fn string(&mut self, bytes: &[u8]) {
        if self.flags.numeric_check {
            let parsed = parse_numeric(bytes);
            if parsed.exact {
                match parsed.value {
                    Some(NumVal::Int(integer)) => {
                        self.output.push_str(&integer.to_string());
                        return;
                    }
                    Some(NumVal::Float(number)) if number.is_finite() => {
                        self.number(number);
                        return;
                    }
                    _ => {}
                }
            }
        }
        let start = self.output.len();
        if let Err(error) = encode_string(&mut self.output, bytes, self.flags) {
            self.output.truncate(start);
            self.failure(error);
        }
    }
}
