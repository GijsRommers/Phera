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
use crate::string::encode_string;
use phera_extension_api::value::{NumVal, float_repr, parse_numeric};
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PropertyPurpose,
    PropertyViewContinuation, PropertyViewRequest, Value, args,
};
use std::fmt::Write;
use std::rc::Rc;

mod object;
mod traversal;

use traversal::{Container, Owner, Stack, Work};

struct Encoder {
    flags: EncodeFlags,
    depth: i64,
    output: String,
    error: Error,
    halted: bool,
    work: Stack,
}

struct EncodingView {
    object: Rc<Object>,
    level: usize,
}

struct PendingEncoding {
    encoder: Encoder,
    view: EncodingView,
}

impl PropertyViewContinuation for PendingEncoding {
    fn resume(
        self: Box<Self>,
        context: &mut dyn Context,
        properties: Rc<PhpArray>,
    ) -> LocalFuture<'_, NativeResult> {
        let Self { mut encoder, view } = *self;
        encoder.object_entries(view.object, view.level, properties);
        Box::pin(complete_encoding(context, encoder))
    }

    fn abandon(self: Box<Self>, context: &mut dyn Context) {
        self.encoder.record_exception(context);
    }
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
    let mut encoder = Encoder::new(&arguments);
    encoder.work.push(Work::value(
        arguments.first().cloned().unwrap_or_default(),
        0,
    ));
    complete_encoding(context, encoder).await
}

pub(crate) fn can_encode_sync(slots: &[Option<Value>]) -> bool {
    match slots.first().and_then(Option::as_ref) {
        Some(Value::Array(array)) => array.values().all(plain_value),
        Some(value) => plain_value(value),
        None => false,
    }
}

fn plain_value(value: &Value) -> bool {
    !matches!(
        value,
        Value::Object(_) | Value::Reference(_) | Value::Array(_)
    )
}

pub(crate) fn encode_sync(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut encoder = Encoder::new(arguments);
    if let Some(value) = arguments.first() {
        encoder.value(value, 0);
    }
    if encoder.advance().is_some() {
        return Err(NativeError::error(
            "Object encoding requires the native call frame",
        ));
    }
    encoder.finish(context)
}

async fn complete_encoding(context: &mut dyn Context, mut encoder: Encoder) -> NativeResult {
    while let Some((object, level)) = encoder.advance() {
        match encoder.object(context, object, level).await {
            Ok(Some(view)) => {
                return Err(PropertyViewRequest {
                    object: view.object.clone(),
                    purpose: PropertyPurpose::Json,
                    function: "json_encode",
                    arguments: Vec::new(),
                    continuation: Box::new(PendingEncoding { encoder, view }),
                }
                .suspend());
            }
            Ok(None) => {}
            Err(exception) => {
                encoder.record_exception(context);
                return Err(exception);
            }
        }
    }
    encoder.finish(context)
}

impl Encoder {
    fn new(arguments: &[Value]) -> Self {
        Self {
            flags: EncodeFlags::from_bits(args::int(arguments, 1).unwrap_or(0)),
            // PHP keeps the depth in a C int.
            depth: i64::from(args::int(arguments, 2).unwrap_or(512) as i32),
            output: String::new(),
            error: Error::None,
            halted: false,
            work: Stack::default(),
        }
    }

    fn finish(self, context: &mut dyn Context) -> NativeResult {
        let result = if self.error == Error::None || self.flags.partial_output {
            Value::from(self.output)
        } else {
            Value::Bool(false)
        };
        error::finish(
            context,
            self.error,
            self.flags.throw_on_error && !self.flags.partial_output,
            result,
        )
    }

    fn record_exception(&self, context: &mut dyn Context) {
        if !self.flags.throw_on_error || self.flags.partial_output {
            let _ = error::finish(context, self.error, false, Value::Null);
        }
    }

    /// Writes until done or until an object needs services from the runtime.
    fn advance(&mut self) -> Option<(Rc<Object>, usize)> {
        while !self.halted {
            match self.work.pop()? {
                Work::Value(Value::Object(object), level) => return Some((object, level)),
                Work::Value(value, level) => self.value(&value, level),
                Work::ReleaseSerializer(guard) => drop(guard),
                Work::ReleaseReference(reference) => {
                    self.work.leave(Rc::as_ptr(&reference) as usize);
                }
                Work::Container(mut container) => {
                    let level = container.level;
                    let object = container.object;
                    if let Some((key, value)) = container.next_entry() {
                        if container.wrote_entry {
                            self.output.push(',');
                        }
                        container.wrote_entry = true;
                        self.indent(level);
                        if object {
                            self.key(key);
                            self.output
                                .push_str(if self.flags.pretty_print { ": " } else { ":" });
                        }
                        self.work.push(Work::Container(container));
                        // A bad key records an error, but its value still runs.
                        if let Value::Object(object) = value {
                            return Some((object, level));
                        }
                        self.value(&value, level);
                    } else {
                        self.work.leave(container.identity());
                        if level as i64 > self.depth {
                            self.error = Error::Depth;
                            self.halted = !self.flags.partial_output;
                        }
                        if container.wrote_entry {
                            self.indent(level - 1);
                        }
                        self.output.push(if object { '}' } else { ']' });
                    }
                }
            }
        }
        None
    }

    /// Records an error and writes what PHP writes in place of the value:
    /// 0 for a float that is not finite, null for anything else.
    fn failure(&mut self, error: Error) {
        self.error = error;
        // Non-finite numbers still yield a placeholder and let later values run.
        self.halted = !self.flags.partial_output && error != Error::Infinite;
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
                let _ = write!(self.output, "{integer}");
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

    fn value(&mut self, value: &Value, level: usize) {
        match value {
            Value::Null => self.output.push_str("null"),
            Value::Bool(boolean) => self
                .output
                .push_str(if *boolean { "true" } else { "false" }),
            Value::Int(integer) => {
                let _ = write!(self.output, "{integer}");
            }
            Value::Float(number) => self.number(*number),
            Value::String(bytes) => self.string(bytes.as_bytes()),
            Value::Resource(_) => self.failure(Error::Unsupported),
            Value::Reference(reference) => {
                let identity = Rc::as_ptr(reference) as usize;
                if self.work.contains(identity) {
                    self.failure(Error::Recursion);
                    return;
                }
                // A reference already borrowed for writing is one being changed further up.
                if let Ok(value) = reference.try_borrow() {
                    self.work
                        .push_active(identity, Work::ReleaseReference(reference.clone()));
                    self.work.push(Work::value(value.clone(), level));
                } else {
                    self.failure(Error::Recursion);
                }
            }
            Value::Array(array) => {
                let object = !array.is_list() || self.flags.force_object;
                self.container(Owner::Array(array.clone()), object, level);
            }
            Value::Object(object) => self
                .work
                .push(Work::Value(Value::Object(object.clone()), level)),
        }
    }

    fn container(&mut self, owner: Owner, object: bool, level: usize) {
        let identity = owner.identity();
        if self.work.contains(identity) {
            self.failure(Error::Recursion);
            return;
        }
        self.output.push(if object { '{' } else { '[' });
        self.work.push_active(
            identity,
            Work::Container(Container::new(owner, object, level + 1)),
        );
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
                        let _ = write!(self.output, "{integer}");
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
