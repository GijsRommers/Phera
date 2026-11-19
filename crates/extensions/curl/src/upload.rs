//! `curl_file_create('photo.png')` describes a multipart upload part.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ConstantValue, Context, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};

const FILE: &[Param] = &[
    Param::new("filename", ParamType::String),
    Param::new("mime_type", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
    Param::new("posted_filename", ParamType::String)
        .nullable()
        .with_default(ConstantValue::Null),
];
const STRING_FILE: &[Param] = &[
    Param::new("data", ParamType::String),
    Param::new("postname", ParamType::String),
    Param::new("mime", ParamType::String)
        .with_default(ConstantValue::String("application/octet-stream")),
];

const fn property(name: &'static str) -> PropertyEntry {
    PropertyEntry::typed(name, PropertyType::String, Visibility::Public)
}

pub(crate) fn register(registry: &mut Registry) {
    const EMPTY: ConstantValue = ConstantValue::String("");
    const UPLOAD: ClassEntry = ClassEntry {
        properties: &[
            property("name").with_default(EMPTY),
            property("mime").with_default(EMPTY),
            property("postname").with_default(EMPTY),
        ],
        methods: &[
            MethodEntry::public("__construct", FILE, construct_file),
            MethodEntry::public("getFilename", &[], filename).tentative_return("string"),
            MethodEntry::public("getMimeType", &[], mime).tentative_return("string"),
            MethodEntry::public("getPostFilename", &[], postname).tentative_return("string"),
            MethodEntry::public(
                "setMimeType",
                &[Param::new("mime_type", ParamType::String)],
                set_mime,
            )
            .tentative_return("void"),
            MethodEntry::public(
                "setPostFilename",
                &[Param::new("posted_filename", ParamType::String)],
                set_postname,
            )
            .tentative_return("void"),
        ],
        ..ClassEntry::class("CURLFile")
    };
    const STRING_UPLOAD: ClassEntry = ClassEntry {
        properties: &[property("data"), property("postname"), property("mime")],
        methods: &[MethodEntry::public(
            "__construct",
            STRING_FILE,
            construct_string_file,
        )],
        ..ClassEntry::class("CURLStringFile")
    };
    registry.class(UPLOAD);
    registry.class(STRING_UPLOAD);
    registry.function(phera_extension_api::FunctionEntry::sync(
        "curl_file_create",
        FILE,
        create,
    ));
}

fn file_properties(function: &str, values: &[Value]) -> Result<PhpArray, NativeError> {
    if args::bytes(values, 0).contains(&0) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($filename) must not contain any null bytes"
        )));
    }
    let mut properties = PhpArray::new();
    for (index, name) in ["name", "mime", "postname"].into_iter().enumerate() {
        properties.insert(
            ArrayKey::str(name),
            Value::from(args::bytes(values, index).to_vec()),
        );
    }
    Ok(properties)
}

fn create(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    context.create_object("CURLFile", file_properties("curl_file_create", values)?)
}

fn construct_file(
    _: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
) -> NativeResult {
    let properties = file_properties("CURLFile::__construct", values)?;
    if let Some(object) = object {
        for (key, value) in properties.iter() {
            object.set(key, value.clone());
        }
    }
    Ok(Value::Null)
}

fn construct_string_file(
    _: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
) -> NativeResult {
    if let Some(object) = object {
        for (index, name) in ["data", "postname", "mime"].into_iter().enumerate() {
            let value = match args::value(values, index) {
                Value::String(value) => Value::String(value.clone()),
                _ => Value::from("application/octet-stream"),
            };
            object.set(ArrayKey::str(name), value);
        }
    }
    Ok(Value::Null)
}

fn get(object: Option<&Rc<Object>>, name: &str) -> Value {
    object
        .and_then(|object| object.get(&ArrayKey::str(name)))
        .unwrap_or_else(|| Value::from(""))
}

fn set(object: Option<&Rc<Object>>, name: &str, values: &[Value]) -> NativeResult {
    if let Some(object) = object {
        object.set(ArrayKey::str(name), args::value(values, 0).deref().clone());
    }
    Ok(Value::Null)
}

fn filename(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(get(object, "name"))
}

fn mime(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(get(object, "mime"))
}

fn postname(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(get(object, "postname"))
}

fn set_mime(_: &mut dyn Context, object: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    set(object, "mime", values)
}

fn set_postname(
    _: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
) -> NativeResult {
    set(object, "postname", values)
}
