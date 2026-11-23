//! A user filter takes a mutable `StreamBucket` from its input brigade.

use std::{cell::RefCell, collections::VecDeque, rc::Weak};

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, FunctionEntry, NativeError,
    NativeResult, Param, ParamType, PhpArray, PhpString, PropertyEntry, PropertyType, Registry,
    Value, Visibility, args, value::Resource,
};

use crate::request::new_resource;
use crate::resource::{argument_resource, stream_arg};

const BRIGADE: Param = Param::new("brigade", ParamType::Mixed);
const CREATE: &[Param] = &[
    crate::handles::STREAM,
    Param::new("buffer", ParamType::String),
];
const BUCKET: Param = Param::new("bucket", ParamType::Class("StreamBucket"));

pub(super) struct Brigade {
    buckets: VecDeque<Bucket>,
    size: usize,
}

struct Bucket {
    id: Option<i64>,
    data: PhpString,
}

#[derive(Default)]
struct BucketLink(RefCell<Weak<Resource>>);

pub(super) fn brigade(context: &mut dyn Context, input: Vec<u8>) -> Value {
    let size = input.len();
    let buckets = if input.is_empty() {
        VecDeque::new()
    } else {
        VecDeque::from([Bucket {
            id: None,
            data: PhpString::from(input),
        }])
    };
    new_resource(
        context.state(),
        "userfilter.bucket brigade",
        Brigade { buckets, size },
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
        self.buckets.is_empty()
    }

    pub(super) fn bytes(self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.size);
        for bucket in self.buckets {
            result.extend_from_slice(bucket.data.as_bytes());
        }
        result
    }

    fn unlink(&mut self, id: i64) {
        if let Some(index) = self.buckets.iter().position(|bucket| bucket.id == Some(id))
            && let Some(bucket) = self.buckets.remove(index)
        {
            self.size -= bucket.data.as_bytes().len();
        }
    }
}

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        properties: const {
            &[
                PropertyEntry::new("bucket", ConstantValue::Null, Visibility::Public),
                PropertyEntry::typed("data", PropertyType::String, Visibility::Public),
                PropertyEntry::typed("datalen", PropertyType::Int, Visibility::Public),
                PropertyEntry::typed("dataLength", PropertyType::Int, Visibility::Public),
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
    let resource = brigade_argument(arguments, "stream_bucket_make_writeable")?;
    let data = use_brigade(resource, "stream_bucket_make_writeable", |brigade| {
        let bucket = brigade.buckets.pop_front()?;
        brigade.size -= bucket.data.as_bytes().len();
        Some(bucket.data)
    })?;
    data.map_or(Ok(Value::Null), |bytes| make_bucket(context, bytes))
}

fn create(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    stream_arg(arguments, "stream_bucket_new")?;
    let data = args::bytes(arguments, 1);
    super::super::bounded(data.len()).map_err(|error| NativeError::error(error.to_string()))?;
    make_bucket(context, PhpString::from(data.to_vec()))
}

fn make_bucket(context: &mut dyn Context, data: PhpString) -> NativeResult {
    let mut properties = PhpArray::new();
    let len = Value::Int(data.as_bytes().len() as i64);
    let bucket = new_resource(context.state(), "userfilter.bucket", BucketLink::default());
    properties.insert(ArrayKey::str("bucket"), bucket);
    properties.insert(ArrayKey::str("data"), Value::String(data));
    properties.insert(ArrayKey::str("datalen"), len.clone());
    properties.insert(ArrayKey::str("dataLength"), len);
    context.create_object("StreamBucket", properties)
}

fn brigade_argument<'a>(
    arguments: &'a [Value],
    function: &str,
) -> Result<&'a std::rc::Rc<Resource>, NativeError> {
    argument_resource(args::value(arguments, 0), function, "#1 ($brigade)")
}

fn use_brigade<T>(
    resource: &Resource,
    function: &str,
    work: impl FnOnce(&mut Brigade) -> T,
) -> Result<T, NativeError> {
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
    let value = bucket.get(&ArrayKey::str("data")).ok_or_else(|| {
        NativeError::error(
            "Typed property StreamBucket::$data must not be accessed before initialization",
        )
    })?;
    let Some(data) = value.deref().to_php_string() else {
        return Err(NativeError::error("Stream bucket data must be a string"));
    };
    let handle = bucket.get(&ArrayKey::str("bucket")).unwrap_or(Value::Null);
    let handle = handle.deref();
    let Value::Resource(handle) = &*handle else {
        return Err(NativeError::type_error(format!(
            "{function}(): supplied resource is not a valid stream bucket resource"
        )));
    };
    let handle_data = handle.data.borrow();
    let Some(link) = handle_data
        .as_ref()
        .and_then(|data| data.downcast_ref::<BucketLink>())
    else {
        return Err(NativeError::type_error(format!(
            "{function}(): supplied resource is not a valid stream bucket resource"
        )));
    };
    let target = brigade_argument(arguments, function)?;
    let previous = link.0.borrow().upgrade();
    if let Some(previous) = previous
        && !previous.is_closed()
    {
        use_brigade(&previous, function, |brigade| brigade.unlink(handle.id))?;
    }
    use_brigade(target, function, |brigade| {
        // Account for queue entries even when a filter appends empty buckets.
        if brigade.buckets.len() >= super::super::MAX_BUCKET / std::mem::size_of::<Bucket>() {
            return Err(super::super::limit());
        }
        let n = brigade
            .size
            .checked_add(data.as_bytes().len())
            .ok_or_else(super::super::limit)?;
        super::super::bounded(n)?;
        let bucket = Bucket {
            id: Some(handle.id),
            data: data.clone(),
        };
        if prepend {
            brigade.buckets.push_front(bucket);
        } else {
            brigade.buckets.push_back(bucket);
        }
        brigade.size = n;
        Ok::<_, std::io::Error>(())
    })?
    .map_err(|error| NativeError::error(error.to_string()))?;
    *link.0.borrow_mut() = std::rc::Rc::downgrade(target);
    Ok(Value::Null)
}
