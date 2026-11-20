//! A user filter takes a mutable `StreamBucket` from its input brigade.

use std::collections::VecDeque;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, FunctionEntry, NativeError,
    NativeResult, Param, ParamType, PhpArray, PropertyEntry, Registry, Value, Visibility, args,
};

use crate::request::new_resource;
use crate::resource::{argument_resource, stream_arg};

const BRIGADE: Param = Param::new("brigade", ParamType::Mixed);
const CREATE: &[Param] = &[
    crate::handles::STREAM,
    Param::new("buffer", ParamType::String),
];
const BUCKET: Param = Param::new("bucket", ParamType::Class("StreamBucket"));

pub(super) struct Brigade(VecDeque<Vec<u8>>, usize);

pub(super) fn brigade(context: &mut dyn Context, input: Vec<u8>) -> Value {
    let size = input.len();
    let buckets = if input.is_empty() {
        VecDeque::new()
    } else {
        VecDeque::from([input])
    };
    new_resource(
        context.state(),
        "userfilter.bucket brigade",
        Brigade(buckets, size),
    )
}

pub(super) fn finish(value: &Value) -> Option<Brigade> {
    let Value::Resource(resource) = value else {
        return None;
    };
    resource
        .data
        .borrow_mut()
        .take()
        .and_then(|data| data.downcast::<Brigade>().ok())
        .map(|data| *data)
}

impl Brigade {
    pub(super) fn empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.1);
        result.extend(self.0.into_iter().flatten());
        result
    }
}

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        properties: const {
            &[
                PropertyEntry::new("bucket", ConstantValue::Null, Visibility::Public).readonly(),
                PropertyEntry::new("data", ConstantValue::String(""), Visibility::Public),
                PropertyEntry::new("datalen", ConstantValue::Int(0), Visibility::Public),
                PropertyEntry::new("dataLength", ConstantValue::Int(0), Visibility::Public),
            ]
        },
        ..ClassEntry::class("StreamBucket")
    });
    for entry in const {
        [
            FunctionEntry::sync("stream_bucket_make_writeable", &[BRIGADE], make_writeable),
            FunctionEntry::sync("stream_bucket_append", &[BRIGADE, BUCKET], append),
            FunctionEntry::sync("stream_bucket_prepend", &[BRIGADE, BUCKET], prepend),
            FunctionEntry::sync("stream_bucket_new", CREATE, create),
        ]
    } {
        registry.function(entry);
    }
}

fn make_writeable(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let data = use_brigade(arguments, "stream_bucket_make_writeable", |brigade| {
        let bucket = brigade.0.pop_front()?;
        brigade.1 -= bucket.len();
        Some(bucket)
    })?;
    data.map_or(Ok(Value::Null), |bytes| make_bucket(context, bytes))
}

fn create(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    stream_arg(arguments, "stream_bucket_new")?;
    let data = args::bytes(arguments, 1);
    super::super::bounded(data.len()).map_err(|error| NativeError::error(error.to_string()))?;
    make_bucket(context, data.to_vec())
}

fn make_bucket(context: &mut dyn Context, data: Vec<u8>) -> NativeResult {
    let mut properties = PhpArray::new();
    let len = Value::Int(data.len() as i64);
    let bucket = new_resource(context.state(), "userfilter.bucket", ());
    properties.insert(ArrayKey::str("bucket"), bucket);
    properties.insert(ArrayKey::str("data"), Value::from(data));
    properties.insert(ArrayKey::str("datalen"), len.clone());
    properties.insert(ArrayKey::str("dataLength"), len);
    context.create_object("StreamBucket", properties)
}

fn use_brigade<T>(
    arguments: &[Value],
    function: &str,
    work: impl FnOnce(&mut Brigade) -> T,
) -> Result<T, NativeError> {
    let resource = argument_resource(args::value(arguments, 0), function, "#1 ($brigade)")?;
    let mut data = resource.data.borrow_mut();
    let Some(brigade) = data
        .as_mut()
        .and_then(|data| data.downcast_mut::<Brigade>())
    else {
        return Err(NativeError::type_error(format!(
            "{function}(): supplied resource is not a valid stream bucket brigade resource"
        )));
    };
    Ok(work(brigade))
}

fn append(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    insert(arguments, "stream_bucket_append", false)
}

fn prepend(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    insert(arguments, "stream_bucket_prepend", true)
}

fn insert(arguments: &[Value], function: &str, prepend: bool) -> NativeResult {
    let Value::Object(bucket) = args::value(arguments, 1) else {
        return Err(NativeError::error(
            "Stream bucket argument did not contain an object",
        ));
    };
    let Some(Value::String(data)) = bucket.get(&ArrayKey::str("data")) else {
        return Err(NativeError::error("Stream bucket data must be a string"));
    };
    let data = data.as_bytes();
    use_brigade(arguments, function, |brigade| {
        let n = brigade
            .1
            .checked_add(data.len())
            .ok_or_else(super::super::limit)?;
        super::super::bounded(n)?;
        if prepend {
            brigade.0.push_front(data.to_vec());
        } else {
            brigade.0.push_back(data.to_vec());
        }
        brigade.1 = n;
        Ok::<_, std::io::Error>(())
    })?
    .map_err(|error| NativeError::error(error.to_string()))?;
    Ok(Value::Null)
}
