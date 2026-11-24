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
use phera_extension_api::value::{NumVal, parse_numeric, push_integer, with_float_repr};
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeResult, Object, PhpArray, PropertyPurpose,
    PropertyViewContinuation, PropertyViewRequest, Value, args,
};
use std::cell::RefCell;
use std::rc::Rc;

mod object;
mod traversal;

use traversal::{Container, Owner, Stack, Work};

const SYNC_MAX_DEPTH: usize = 8;
const SYNC_MAX_INSPECTED: usize = 256;

struct Encoder {
    flags: EncodeFlags,
    depth: i64,
    output: Vec<u8>,
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
    let Some(value) = slots.first().and_then(Option::as_ref) else {
        return false;
    };
    let mut remaining = SYNC_MAX_INSPECTED;
    can_encode_sync_value(value, 0, &mut remaining)
}

fn can_encode_sync_value(value: &Value, depth: usize, remaining: &mut usize) -> bool {
    match value {
        Value::Object(_) | Value::Reference(_) => false,
        Value::Array(array) => {
            if depth >= SYNC_MAX_DEPTH || array.len() > *remaining {
                return false;
            }
            *remaining -= array.len();
            for item in array.values() {
                if !can_encode_sync_value(item, depth + 1, remaining) {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

pub(crate) fn encode_sync(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut encoder = Encoder::new(arguments);
    if let Some(value) = arguments.first() {
        encoder.encode_value_borrowed(value, 0);
    }
    encoder.finish(context)
}

fn initial_capacity(value: Option<&Value>) -> usize {
    match value {
        Some(Value::Array(array)) => array.len().saturating_mul(32).clamp(64, 65_536),
        Some(Value::Object(_)) => 128,
        Some(Value::String(bytes)) => bytes.len().saturating_add(2).clamp(32, 65_536),
        Some(Value::InternedStr(bytes)) => bytes.len().saturating_add(2).clamp(32, 65_536),
        _ => 32,
    }
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
            output: Vec::with_capacity(initial_capacity(arguments.first())),
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
                    let colon: &[u8] = if self.flags.pretty_print { b": " } else { b":" };
                    let mut suspended = None;
                    while !self.halted {
                        if let Some((comma, key, value)) = container.next_entry() {
                            if comma {
                                self.output.push(b',');
                            }
                            self.indent(level);
                            if object {
                                self.key(&key);
                                self.output.extend_from_slice(colon);
                            }
                            // A bad key records an error, but its value still runs.
                            match value {
                                Value::Object(child) => {
                                    let child = child.clone();
                                    self.work.push(Work::Container(container));
                                    suspended = Some((child, level));
                                    break;
                                }
                                Value::Array(array) => {
                                    let array = array.clone();
                                    self.work.push(Work::Container(container));
                                    let child_object = !array.is_list() || self.flags.force_object;
                                    self.container(Owner::Array(array), child_object, level);
                                    break;
                                }
                                Value::Reference(reference) => {
                                    let reference = reference.clone();
                                    self.work.push(Work::Container(container));
                                    self.reference(&reference, level);
                                    break;
                                }
                                _ => self.primitive(value),
                            }
                        } else {
                            self.work.leave(container.identity());
                            if level as i64 > self.depth {
                                self.error = Error::Depth;
                                self.halted = !self.flags.partial_output;
                            }
                            if container.wrote_entry {
                                self.indent(level - 1);
                            }
                            self.output.push(if object { b'}' } else { b']' });
                            break;
                        }
                    }
                    if let Some(object) = suspended {
                        return Some(object);
                    }
                }
            }
        }
        None
    }

    fn encode_value_borrowed(&mut self, value: &Value, level: usize) {
        match value {
            Value::Array(array) => self.encode_array_borrowed(array, level),
            Value::Object(_) | Value::Reference(_) => {
                unreachable!("can_encode_sync excludes objects and references")
            }
            _ => self.primitive(value),
        }
    }

    fn encode_array_borrowed(&mut self, array: &PhpArray, depth: usize) {
        debug_assert!(depth < SYNC_MAX_DEPTH);
        let object = !array.is_list() || self.flags.force_object;
        let entry_level = depth + 1;
        self.output.push(if object { b'{' } else { b'[' });
        let mut wrote_entry = false;
        if object {
            let colon: &[u8] = if self.flags.pretty_print { b": " } else { b":" };
            for (key, value) in array.iter() {
                if wrote_entry {
                    self.output.push(b',');
                }
                wrote_entry = true;
                self.indent(entry_level);
                self.key(&key);
                self.output.extend_from_slice(colon);
                self.encode_value_borrowed(value, entry_level);
                if self.halted {
                    return;
                }
            }
        } else {
            for value in array.values() {
                if wrote_entry {
                    self.output.push(b',');
                }
                wrote_entry = true;
                self.indent(entry_level);
                self.encode_value_borrowed(value, entry_level);
                if self.halted {
                    return;
                }
            }
        }
        if entry_level as i64 > self.depth {
            self.error = Error::Depth;
            self.halted = !self.flags.partial_output;
        }
        if wrote_entry {
            self.indent(depth);
        }
        self.output.push(if object { b'}' } else { b']' });
    }

    /// Records an error and writes what PHP writes in place of the value:
    /// 0 for a float that is not finite, null for anything else.
    fn failure(&mut self, error: Error) {
        self.error = error;
        // Non-finite numbers still yield a placeholder and let later values run.
        self.halted = !self.flags.partial_output && error != Error::Infinite;
        self.output
            .extend_from_slice(if matches!(error, Error::Infinite | Error::NonBackedEnum) {
                b"0"
            } else {
                b"null"
            });
    }

    fn indent(&mut self, level: usize) {
        if self.flags.pretty_print {
            self.output.push(b'\n');
            for _ in 0..level {
                self.output.extend_from_slice(b"    ");
            }
        }
    }

    /// A key that is not UTF-8 is written as `""`, not null.
    fn key(&mut self, key: &ArrayKey) {
        match key {
            ArrayKey::Int(integer) => {
                self.output.push(b'"');
                push_integer(&mut self.output, *integer);
                self.output.push(b'"');
            }
            ArrayKey::Interned(bytes) => self.string_key(bytes.as_bytes()),
            ArrayKey::String(bytes) => self.string_key(bytes.as_bytes()),
        }
    }

    fn string_key(&mut self, bytes: &[u8]) {
        let start = self.output.len();
        if let Err(error) = encode_string(&mut self.output, bytes, self.flags) {
            self.output.truncate(start);
            self.error = error;
            self.output.extend_from_slice(b"\"\"");
        }
    }

    fn primitive(&mut self, value: &Value) {
        match value {
            Value::Null => self.output.extend_from_slice(b"null"),
            Value::Bool(boolean) => {
                self.output
                    .extend_from_slice(if *boolean { b"true" } else { b"false" })
            }
            Value::Int(integer) => push_integer(&mut self.output, *integer),
            Value::Float(number) => self.number(*number),
            Value::InternedStr(bytes) => self.string(bytes.as_bytes()),
            Value::String(bytes) => self.string(bytes.as_bytes()),
            Value::Resource(_) => self.failure(Error::Unsupported),
            Value::Reference(_) | Value::Array(_) | Value::Object(_) => {
                unreachable!("primitive only handles scalar and resource values")
            }
        }
    }

    fn reference(&mut self, reference: &Rc<RefCell<Value>>, level: usize) {
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

    fn value(&mut self, value: &Value, level: usize) {
        match value {
            Value::Reference(reference) => self.reference(reference, level),
            Value::Array(array) => {
                let object = !array.is_list() || self.flags.force_object;
                self.container(Owner::Array(array.clone()), object, level);
            }
            Value::Object(object) => self
                .work
                .push(Work::Value(Value::Object(object.clone()), level)),
            _ => self.primitive(value),
        }
    }

    fn container(&mut self, owner: Owner, object: bool, level: usize) {
        let identity = owner.identity();
        if self.work.contains(identity) {
            self.failure(Error::Recursion);
            return;
        }
        self.output.push(if object { b'{' } else { b'[' });
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
        let preserve_zero_fraction = self.flags.preserve_zero_fraction;
        let out = &mut self.output;
        with_float_repr(number, -1, |text| {
            let bytes = text.as_bytes();
            let mut has_dot_or_exp = false;
            if let Some(exp_pos) = bytes.iter().position(|&byte| byte == b'E') {
                has_dot_or_exp = true;
                out.extend_from_slice(&bytes[..exp_pos]);
                out.push(b'e');
                out.extend_from_slice(&bytes[exp_pos + 1..]);
            } else {
                out.extend_from_slice(bytes);
                if preserve_zero_fraction {
                    has_dot_or_exp = bytes.iter().any(|&byte| byte == b'.' || byte == b'e');
                }
            }
            if preserve_zero_fraction && !has_dot_or_exp {
                out.extend_from_slice(b".0");
            }
        });
    }

    /// With `JSON_NUMERIC_CHECK` a numeric string is written as a number.
    /// Keys are never checked.
    fn string(&mut self, bytes: &[u8]) {
        if self.flags.numeric_check {
            let parsed = parse_numeric(bytes);
            if parsed.exact {
                match parsed.value {
                    Some(NumVal::Int(integer)) => {
                        push_integer(&mut self.output, integer);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags;
    use phera_extension_api::testing::TestContext;
    use phera_extension_api::{ArrayKey, NativeError, PhpArray, Value};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::task::{Context as TaskContext, Poll, Waker};

    fn run_async(args: &[Value]) -> (NativeResult, Error) {
        let mut ctx = TestContext::default();
        let result = match json_encode(&mut ctx, args.to_vec())
            .as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("async json_encode unexpectedly suspended"),
        };
        let err = *ctx.state().get_or_default::<Error>();
        (result, err)
    }

    fn run_sync(args: &[Value]) -> (NativeResult, Error) {
        let mut ctx = TestContext::default();
        let result = encode_sync(&mut ctx, args);
        let err = *ctx.state().get_or_default::<Error>();
        (result, err)
    }

    fn assert_same_result(sync_res: NativeResult, async_res: NativeResult, args: &[Value]) {
        match (sync_res, async_res) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "value mismatch for {args:?}"),
            (
                Err(NativeError::Throw {
                    class: c1,
                    message: m1,
                    code: k1,
                }),
                Err(NativeError::Throw {
                    class: c2,
                    message: m2,
                    code: k2,
                }),
            ) => assert_eq!((c1, m1, k1), (c2, m2, k2), "throw mismatch for {args:?}"),
            (a, b) => panic!("result mismatch for {args:?}: sync={a:?}, async={b:?}"),
        }
    }

    fn assert_sync_matches_async(args: &[Value]) {
        let slots: Vec<Option<Value>> = args.iter().cloned().map(Some).collect();
        assert!(can_encode_sync(&slots), "expected sync path for {args:?}");
        let (sync_res, sync_err) = run_sync(args);
        let (async_res, async_err) = run_async(args);
        assert_same_result(sync_res, async_res, args);
        assert_eq!(sync_err, async_err, "error mismatch for {args:?}");
    }

    fn nested_list(depth: usize, leaf: Value) -> Value {
        let mut current = leaf;
        for _ in 0..depth {
            let mut array = PhpArray::new();
            array.push(current).unwrap();
            current = Value::from(array);
        }
        current
    }

    #[test]
    fn sync_selector_bounds_depth_and_entries() {
        assert!(!can_encode_sync(&[]));
        assert!(can_encode_sync(&[Some(Value::Int(42))]));
        assert!(can_encode_sync(&[Some(nested_list(
            SYNC_MAX_DEPTH,
            Value::Int(1)
        ))]));
        assert!(!can_encode_sync(&[Some(nested_list(
            SYNC_MAX_DEPTH + 1,
            Value::Int(1)
        ))]));

        let mut at_budget = PhpArray::with_capacity(SYNC_MAX_INSPECTED);
        for i in 0..SYNC_MAX_INSPECTED {
            at_budget.push(Value::Int(i as i64)).unwrap();
        }
        assert!(can_encode_sync(&[Some(Value::from(at_budget))]));

        let mut over_budget = PhpArray::with_capacity(SYNC_MAX_INSPECTED + 1);
        for i in 0..=SYNC_MAX_INSPECTED {
            over_budget.push(Value::Int(i as i64)).unwrap();
        }
        assert!(!can_encode_sync(&[Some(Value::from(over_budget))]));

        let mut with_ref = PhpArray::new();
        let mut inner = PhpArray::new();
        inner
            .push(Value::Reference(Rc::new(RefCell::new(Value::Int(1)))))
            .unwrap();
        with_ref.push(Value::from(inner)).unwrap();
        assert!(!can_encode_sync(&[Some(Value::from(with_ref))]));
    }

    #[test]
    fn nested_array_sync_matches_async_across_flags() {
        let mut row1 = PhpArray::new();
        row1.insert(ArrayKey::interned(b"id"), Value::Int(1));
        row1.insert(ArrayKey::interned(b"score"), Value::Float(1.0));
        row1.insert(ArrayKey::interned(b"big"), Value::Float(1e25));
        row1.insert(ArrayKey::interned(b"neg_zero"), Value::Float(-0.0));
        row1.insert(ArrayKey::interned(b"num_str"), Value::from("42"));
        row1.insert(ArrayKey::interned(b"float_str"), Value::from("3.5"));
        row1.insert(ArrayKey::interned(b"active"), Value::Bool(true));
        row1.insert(ArrayKey::interned(b"meta"), Value::Null);
        row1.insert(
            ArrayKey::interned(b"html"),
            Value::from("https://a.test/x&y<'z'>\"é\u{2028}"),
        );
        let mut tags = PhpArray::new();
        tags.push(Value::from("rust")).unwrap();
        tags.push(Value::from("php")).unwrap();
        row1.insert(ArrayKey::interned(b"tags"), Value::from(tags));
        row1.insert(ArrayKey::interned(b"empty"), Value::from(PhpArray::new()));

        let mut rows = PhpArray::new();
        rows.push(Value::from(row1)).unwrap();

        let mut root = PhpArray::new();
        root.insert(ArrayKey::interned(b"rows"), Value::from(rows));
        root.insert(ArrayKey::Int(10), Value::from("ten"));
        let payload = Value::from(root);

        for flag_bits in [
            0,
            flags::FORCE_OBJECT,
            flags::PRETTY_PRINT,
            flags::FORCE_OBJECT | flags::PRETTY_PRINT,
            flags::PRESERVE_ZERO_FRACTION,
            flags::NUMERIC_CHECK,
            flags::UNESCAPED_SLASHES,
            flags::UNESCAPED_UNICODE,
            flags::UNESCAPED_UNICODE | flags::UNESCAPED_LINE_TERMINATORS,
            flags::HEX_TAG | flags::HEX_AMP | flags::HEX_APOS | flags::HEX_QUOT,
            flags::UNESCAPED_SLASHES
                | flags::UNESCAPED_UNICODE
                | flags::HEX_TAG
                | flags::PRETTY_PRINT
                | flags::PRESERVE_ZERO_FRACTION
                | flags::NUMERIC_CHECK,
        ] {
            assert_sync_matches_async(&[payload.clone(), Value::Int(flag_bits)]);
        }
    }

    #[test]
    fn nested_array_sync_matches_async_on_errors_and_depth() {
        let deep = nested_list(3, Value::Int(7));
        for depth in [0, 1, 2, 3, 4] {
            for flag_bits in [
                0,
                flags::PARTIAL_OUTPUT_ON_ERROR,
                flags::THROW_ON_ERROR,
                flags::PARTIAL_OUTPUT_ON_ERROR | flags::THROW_ON_ERROR,
            ] {
                assert_sync_matches_async(&[
                    deep.clone(),
                    Value::Int(flag_bits),
                    Value::Int(depth),
                ]);
            }
        }

        // Invalid UTF-8 in nested key and value, plus NaN/INF ordering.
        let mut bad_map = PhpArray::new();
        bad_map.insert(ArrayKey::str(b"\xffkey".to_vec()), Value::Float(f64::NAN));
        bad_map.insert(ArrayKey::interned(b"ok"), Value::from("after"));
        let mut outer = PhpArray::new();
        outer.push(Value::from(bad_map)).unwrap();
        let mut bad_val = PhpArray::new();
        bad_val.push(Value::Float(f64::INFINITY)).unwrap();
        bad_val.push(Value::from(b"bad\xffutf8".to_vec())).unwrap();
        bad_val.push(Value::Int(99)).unwrap();
        outer.push(Value::from(bad_val)).unwrap();
        let bad_payload = Value::from(outer);

        for flag_bits in [
            0,
            flags::PARTIAL_OUTPUT_ON_ERROR,
            flags::THROW_ON_ERROR,
            flags::INVALID_UTF8_IGNORE,
            flags::INVALID_UTF8_SUBSTITUTE,
            flags::INVALID_UTF8_SUBSTITUTE | flags::PARTIAL_OUTPUT_ON_ERROR,
        ] {
            assert_sync_matches_async(&[bad_payload.clone(), Value::Int(flag_bits)]);
        }
    }
}
