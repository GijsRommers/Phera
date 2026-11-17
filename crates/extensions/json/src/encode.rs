//! Iterative encoding: active identities detect cycles without rejecting shared values.
use crate::{
    error::{self, Error},
    flags::Flags,
    string::encode_string,
};
use phera_extension_api::value::{
    NumVal, PropertyScope, float_repr, parse_numeric, split_property_key,
};
use phera_extension_api::{ArrayKey, Context, NativeResult, Value, args};
use std::collections::HashSet;
use std::rc::Rc;

/// Pending output replaces recursive calls, including reference traversal.
enum Work {
    Value(Value, usize),
    Text(String),
    Key(Vec<u8>),
    End(usize, usize, char, bool),
    Release(usize),
}
/// An encoder owns only clones of input handles, never borrowed RefCells.
struct Encoder {
    flags: Flags,
    depth: i64,
    output: String,
    error: Error,
    active: HashSet<usize>,
    work: Vec<Work>,
}
/// PHP accepts any encode depth; the limit is checked on container completion.
pub(crate) fn json_encode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let flags = Flags::new(args::int(arguments, 1).unwrap_or(0));
    let mut encoder = Encoder {
        flags,
        depth: args::int(arguments, 2).unwrap_or(512) as i32 as i64,
        output: String::new(),
        error: Error::None,
        active: HashSet::new(),
        work: vec![Work::Value(
            arguments.first().cloned().unwrap_or_default(),
            0,
        )],
    };
    encoder.run();
    let result = if encoder.error == Error::None || flags.partial {
        Value::from(encoder.output)
    } else {
        Value::Bool(false)
    };
    error::finish(
        context,
        encoder.error,
        flags.throw && !flags.partial,
        result,
    )
}
impl Encoder {
    /// Keep processing partial output so later failures take PHP's precedence.
    fn run(&mut self) {
        while let Some(work) = self.work.pop() {
            match work {
                Work::Value(value, level) => self.value(value, level),
                Work::Text(text) => self.output.push_str(&text),
                Work::Key(bytes) => match encode_string(&bytes, self.flags) {
                    Ok(text) => self.output.push_str(&text),
                    Err(error) => {
                        self.error = error;
                        self.output.push_str("\"\"");
                    }
                },
                Work::Release(identity) => {
                    self.active.remove(&identity);
                }
                Work::End(identity, level, closing, nonempty) => {
                    self.active.remove(&identity);
                    if level as i64 > self.depth {
                        self.error = Error::Depth;
                    }
                    if self.flags.pretty && nonempty {
                        self.output.push_str(&self.indent(level.saturating_sub(1)));
                    }
                    self.output.push(closing);
                }
            }
            if self.error != Error::None && !self.flags.partial {
                break;
            }
        }
    }
    /// PHP substitutes zero for nonfinite floats and null for other bad values.
    fn failure(&mut self, error: Error) {
        self.error = error;
        self.output.push_str(if error == Error::Infinite {
            "0"
        } else {
            "null"
        });
    }
    fn indent(&self, level: usize) -> String {
        if self.flags.pretty {
            format!("\n{}", "    ".repeat(level))
        } else {
            String::new()
        }
    }
    /// Values and references are dispatched without recursive dereferencing.
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
                if let Ok(value) = reference.try_borrow() {
                    self.work.push(Work::Release(identity));
                    self.work.push(Work::Value(value.clone(), level));
                } else {
                    self.failure(Error::Recursion);
                }
            }
            Value::Array(array) => {
                let entries = array
                    .iter()
                    .map(|(key, value)| (key, value.clone()))
                    .collect();
                self.container(
                    Rc::as_ptr(&array) as usize,
                    entries,
                    !array.is_list() || self.flags.force_object,
                    level,
                );
            }
            Value::Object(object) => {
                let entries = object
                    .to_array()
                    .iter()
                    .filter(|(key, _)| match key {
                        ArrayKey::String(name) => {
                            split_property_key(name.as_bytes()).1 == PropertyScope::Public
                        }
                        _ => true,
                    })
                    .map(|(key, value)| (key, value.clone()))
                    .collect();
                self.container(Rc::as_ptr(&object) as usize, entries, true, level);
            }
        }
    }
    /// Schedule punctuation in reverse order while emitting values in insertion order.
    fn container(
        &mut self,
        identity: usize,
        entries: Vec<(ArrayKey, Value)>,
        object: bool,
        level: usize,
    ) {
        if !self.active.insert(identity) {
            self.failure(Error::Recursion);
            return;
        }
        self.output.push(if object { '{' } else { '[' });
        let count = entries.len();
        self.work.push(Work::End(
            identity,
            level + 1,
            if object { '}' } else { ']' },
            count != 0,
        ));
        for (index, (key, value)) in entries.into_iter().enumerate().rev() {
            self.work.push(Work::Value(value, level + 1));
            if object {
                self.work.push(Work::Text(
                    if self.flags.pretty { ": " } else { ":" }.into(),
                ));
                let bytes = match key {
                    ArrayKey::Int(integer) => integer.to_string().into_bytes(),
                    ArrayKey::String(bytes) => bytes.as_bytes().to_vec(),
                };
                self.work.push(Work::Key(bytes));
            }
            self.work.push(Work::Text(self.indent(level + 1)));
            if index != 0 {
                self.work.push(Work::Text(",".into()));
            }
        }
    }
    /// Shared float formatting reproduces PHP's shortest decimal tie-breaking.
    fn number(&mut self, number: f64) {
        if !number.is_finite() {
            self.failure(Error::Infinite);
            return;
        }
        let mut text = float_repr(number).replace('E', "e");
        if self.flags.fraction && !text.contains(['.', 'e']) {
            text.push_str(".0");
        }
        self.output.push_str(&text);
    }
    /// Numeric checking applies only to values, never property names.
    fn string(&mut self, bytes: &[u8]) {
        if self.flags.numeric {
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
        match encode_string(bytes, self.flags) {
            Ok(text) => self.output.push_str(&text),
            Err(error) => self.failure(error),
        }
    }
}
