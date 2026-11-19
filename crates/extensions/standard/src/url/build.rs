//! PHP form encoding, e.g. `http_build_query(['a' => [true]])` is `a%5B0%5D=1`.

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray,
    QuerySeparator, Value, args, value::to_php_string,
};

use crate::strings::url::{Encoding, encode_into};

pub(super) const ENTRY: FunctionEntry = FunctionEntry::sync(
    "http_build_query",
    &[
        Param::new("data", ParamType::ObjectOrArray),
        Param::new("numeric_prefix", ParamType::String).with_default(ConstantValue::String("")),
        Param::new("arg_separator", ParamType::String)
            .nullable()
            .optional(),
        Param::new("encoding_type", ParamType::Int).optional(),
    ],
    build::<false>,
);

pub(super) const DEPRECATED_OBJECTS_ENTRY: FunctionEntry = FunctionEntry {
    handler: phera_extension_api::NativeFn::Sync(build::<true>),
    ..ENTRY
};

/// The frames own their arrays so that traversal never borrows through PHP references.
struct Frame {
    entries: Rc<PhpArray>,
    next: Option<usize>,
    parent_key_length: usize,
    identity: Identity,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Identity {
    Array(*const PhpArray),
    Object(*const phera_extension_api::value::Object),
}

impl Frame {
    fn new(value: &Value, parent_key_length: usize) -> Option<Self> {
        let (entries, identity) = match &*value.deref() {
            Value::Array(array) => (array.clone(), Identity::Array(Rc::as_ptr(array))),
            Value::Object(object) => (
                Rc::new(object.to_array()),
                Identity::Object(Rc::as_ptr(object)),
            ),
            _ => return None,
        };
        Some(Self {
            next: entries.first_position(),
            entries,
            parent_key_length,
            identity,
        })
    }
}

fn build<const DEPRECATE_OBJECTS: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    let Some(root) = Frame::new(args::value(arguments, 0), 0) else {
        return Ok(Value::from(""));
    };
    if DEPRECATE_OBJECTS && matches!(root.identity, Identity::Object(_)) {
        context.deprecated(concat!(
            "http_build_query(): Passing an object for argument #1 $data to http_build_query() ",
            "is deprecated, call get_object_vars() first instead"
        ).into());
    }
    let numeric_prefix = args::bytes(arguments, 1);
    let separator = args::string(arguments, 2).cloned().unwrap_or_else(|| {
        context
            .state()
            .get::<QuerySeparator>()
            .map_or_else(|| "&".into(), |separator| separator.0.clone())
    });
    let separator = separator.as_bytes();
    let encoding = if args::int(arguments, 3).unwrap_or(1) as i32 == 2 {
        Encoding::Raw
    } else {
        Encoding::Form
    };

    let mut active = HashSet::from([root.identity]);
    let mut frames = vec![root];
    let mut key_bytes = Vec::new();
    let mut query = Vec::new();
    while !frames.is_empty() {
        let nested = frames.len() > 1;
        let Some(frame) = frames.last_mut() else {
            break;
        };
        let Some(position) = frame.next else {
            active.remove(&frame.identity);
            key_bytes.truncate(frame.parent_key_length);
            frames.pop();
            continue;
        };
        frame.next = frame.entries.next_position(position);
        let Some((key, value)) = frame.entries.entry_at(position) else {
            continue;
        };
        let is_object = matches!(frame.identity, Identity::Object(_));
        if is_object && matches!(&key, ArrayKey::String(name) if name.as_bytes().starts_with(b"\0"))
        {
            continue;
        }
        let value = value.try_deref()?;
        if matches!(&*value, Value::Null | Value::Resource(_)) {
            continue;
        }

        let previous = key_bytes.len();
        if nested {
            key_bytes.extend_from_slice(b"%5B");
        }
        match key {
            ArrayKey::Int(index) => {
                if !nested && !is_object {
                    key_bytes.extend_from_slice(numeric_prefix);
                }
                key_bytes.extend_from_slice(index.to_string().as_bytes());
            }
            ArrayKey::String(name) => encode_into(&mut key_bytes, name.as_bytes(), encoding),
        }
        if nested {
            key_bytes.extend_from_slice(b"%5D");
        }

        if let Some(child) = Frame::new(&value, previous) {
            if DEPRECATE_OBJECTS && matches!(child.identity, Identity::Object(_)) {
                context.deprecated(
                    concat!(
                        "http_build_query(): object values within argument #1 $data to ",
                        "http_build_query() being interpreted as arrays is deprecated, instead ",
                        "the $data argument should be preprocessed with get_object_vars()"
                    )
                    .into(),
                );
            }
            if active.insert(child.identity) {
                drop(value);
                frames.push(child);
                continue;
            }
        } else {
            if !query.is_empty() {
                query.extend_from_slice(separator);
            }
            query.extend_from_slice(&key_bytes);
            query.push(b'=');
            match &*value {
                Value::Bool(boolean) => query.push(if *boolean { b'1' } else { b'0' }),
                other => {
                    let string = to_php_string(other, context.diagnostics())?;
                    encode_into(&mut query, string.as_bytes(), encoding);
                }
            }
        }
        key_bytes.truncate(previous);
    }
    Ok(Value::from(query))
}
