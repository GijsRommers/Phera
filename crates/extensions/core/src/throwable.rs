//! `Throwable`, `Exception`, `Error` and the classes derived from them.
//!
//! The VM fills in `file`, `line` and `trace` when it creates one of these,
//! because only it knows where execution is. Everything else is here.

use std::rc::Rc;

use phera_extension_api::value::{float_to_string_precision14, to_php_string};
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeResult, Object,
    Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};

/// The two classes that declare the private properties. A method inherited
/// by `RuntimeException` still finds `previous` under `Exception`'s key.
const ROOTS: [&str; 2] = ["Exception", "Error"];

const NO_PARAMS: &[Param] = &[];

const CONSTRUCT_PARAMS: &[Param] = &[
    Param::new("message", ParamType::String).optional(),
    Param::new("code", ParamType::Int).optional(),
    Param::new("previous", ParamType::Object)
        .nullable()
        .optional(),
];

const ERROR_EXCEPTION_PARAMS: &[Param] = &[
    Param::new("message", ParamType::String).optional(),
    Param::new("code", ParamType::Int).optional(),
    Param::new("severity", ParamType::Int).optional(),
    Param::new("filename", ParamType::String)
        .nullable()
        .optional(),
    Param::new("line", ParamType::Int).nullable().optional(),
    Param::new("previous", ParamType::Object)
        .nullable()
        .optional(),
];

/// In PHP's declaration order, which is the order `var_dump` shows.
const PROPERTIES: &[PropertyEntry] = &[
    PropertyEntry::new("message", ConstantValue::String(""), Visibility::Protected),
    PropertyEntry::typed("string", PropertyType::String, Visibility::Private)
        .with_default(ConstantValue::String("")),
    PropertyEntry::new("code", ConstantValue::Int(0), Visibility::Protected),
    PropertyEntry::typed("file", PropertyType::String, Visibility::Protected)
        .with_default(ConstantValue::String("")),
    PropertyEntry::typed("line", PropertyType::Int, Visibility::Protected)
        .with_default(ConstantValue::Int(0)),
    PropertyEntry::typed("trace", PropertyType::Array, Visibility::Private)
        .with_default(ConstantValue::EmptyArray),
    PropertyEntry::typed(
        "previous",
        PropertyType::NullableClass("Throwable"),
        Visibility::Private,
    )
    .with_default(ConstantValue::Null),
];

const SEVERITY: &[PropertyEntry] =
    &[
        PropertyEntry::typed("severity", PropertyType::Int, Visibility::Protected)
            .with_default(ConstantValue::Int(1)),
    ];

const THROWABLE_METHODS: &[MethodEntry] = &[
    MethodEntry::required("getMessage", NO_PARAMS),
    MethodEntry::required("getCode", NO_PARAMS),
    MethodEntry::required("getFile", NO_PARAMS),
    MethodEntry::required("getLine", NO_PARAMS),
    MethodEntry::required("getTrace", NO_PARAMS),
    MethodEntry::required("getPrevious", NO_PARAMS),
    MethodEntry::required("getTraceAsString", NO_PARAMS),
];

const STRINGABLE_METHODS: &[MethodEntry] = &[MethodEntry::required("__toString", NO_PARAMS)];

/// PHP makes the getters final so that code catching an exception can rely
/// on them.
const METHODS: &[MethodEntry] = &[
    MethodEntry::public("__construct", CONSTRUCT_PARAMS, construct),
    MethodEntry::public("getMessage", NO_PARAMS, get_message).final_(),
    MethodEntry::public("getCode", NO_PARAMS, get_code).final_(),
    MethodEntry::public("getFile", NO_PARAMS, get_file).final_(),
    MethodEntry::public("getLine", NO_PARAMS, get_line).final_(),
    MethodEntry::public("getTrace", NO_PARAMS, get_trace).final_(),
    MethodEntry::public("getPrevious", NO_PARAMS, get_previous).final_(),
    MethodEntry::public("getTraceAsString", NO_PARAMS, get_trace_as_string).final_(),
    MethodEntry::public("__toString", NO_PARAMS, to_string),
    MethodEntry::public("__wakeup", NO_PARAMS, wakeup).tentative_return("void"),
];

const ERROR_EXCEPTION_METHODS: &[MethodEntry] = &[
    MethodEntry::public(
        "__construct",
        ERROR_EXCEPTION_PARAMS,
        construct_error_exception,
    ),
    MethodEntry::public("getSeverity", NO_PARAMS, get_severity).final_(),
];

/// Each class with its parent, parents first.
const DERIVED: &[(&str, &str)] = &[
    ("TypeError", "Error"),
    ("CompileError", "Error"),
    ("ParseError", "CompileError"),
    ("ValueError", "Error"),
    ("ArithmeticError", "Error"),
    ("DivisionByZeroError", "ArithmeticError"),
    ("ArgumentCountError", "TypeError"),
    ("AssertionError", "Error"),
    ("UnhandledMatchError", "Error"),
    ("RuntimeException", "Exception"),
    ("LogicException", "Exception"),
    ("BadFunctionCallException", "LogicException"),
    ("BadMethodCallException", "BadFunctionCallException"),
    ("DomainException", "LogicException"),
    ("InvalidArgumentException", "LogicException"),
    ("LengthException", "LogicException"),
    ("OutOfRangeException", "LogicException"),
    ("OutOfBoundsException", "RuntimeException"),
    ("OverflowException", "RuntimeException"),
    ("RangeException", "RuntimeException"),
    ("UnderflowException", "RuntimeException"),
    ("UnexpectedValueException", "RuntimeException"),
    ("JsonException", "Exception"),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: STRINGABLE_METHODS,
        ..ClassEntry::interface("Stringable")
    });
    registry.class(ClassEntry {
        interfaces: &["Stringable"],
        methods: THROWABLE_METHODS,
        ..ClassEntry::interface("Throwable")
    });
    for root in ROOTS {
        registry.class(ClassEntry {
            interfaces: &["Throwable"],
            properties: PROPERTIES,
            methods: METHODS,
            ..ClassEntry::class(root)
        });
    }
    registry.class(ClassEntry {
        parent: Some("Exception"),
        properties: SEVERITY,
        methods: ERROR_EXCEPTION_METHODS,
        ..ClassEntry::class("ErrorException")
    });
    for &(name, parent) in DERIVED {
        registry.class(ClassEntry {
            kind: ClassKind::Class,
            parent: Some(parent),
            ..ClassEntry::class(name)
        });
    }
}

/// PHP checks the historical untyped fields after deserialization.
fn wakeup(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    if let Some(object) = this {
        let message = Object::protected_key("message");
        if object
            .get(&message)
            .is_some_and(|value| !matches!(value, Value::String(_) | Value::Null))
        {
            object.remove(&message);
        }
        let code = Object::protected_key("code");
        if object
            .get(&code)
            .is_some_and(|value| !matches!(value, Value::Int(_) | Value::Null))
        {
            object.remove(&code);
        }
    }
    Ok(Value::Null)
}

/// Reads a protected property.
fn protected(this: Option<&Rc<Object>>, name: &str) -> Value {
    this.and_then(|object| object.get(&Object::protected_key(name)))
        .unwrap_or(Value::Null)
}

/// Reads a private property, stored under the key of the class that
/// declares it.
fn private(this: Option<&Rc<Object>>, name: &str) -> Value {
    let Some(object) = this else {
        return Value::Null;
    };
    ROOTS
        .iter()
        .find_map(|root| object.get(&Object::private_key(root, name)))
        .unwrap_or(Value::Null)
}

fn set_private(object: &Object, name: &str, value: Value) {
    let key = ROOTS
        .iter()
        .map(|root| Object::private_key(root, name))
        .find(|key| object.has(key))
        .unwrap_or_else(|| Object::private_key(ROOTS[0], name));
    object.set(key, value);
}

/// Only what the caller passed is stored: a subclass may have declared its
/// own default for `$message` or `$code`.
fn construct(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Null);
    };
    if let Some(message) = args::string(arguments, 0) {
        object.set(
            Object::protected_key("message"),
            Value::String(message.clone()),
        );
    }
    if let Some(code) = args::int(arguments, 1) {
        object.set(Object::protected_key("code"), Value::Int(code));
    }
    if let Some(previous) = args::object(arguments, 2) {
        set_private(object, "previous", Value::Object(previous.clone()));
    }
    Ok(Value::Null)
}

fn construct_error_exception(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Null);
    };
    let fields = [
        (0, "message"),
        (1, "code"),
        (2, "severity"),
        (3, "file"),
        (4, "line"),
    ];
    for (index, name) in fields {
        let value = args::value(arguments, index);
        if args::given(arguments, index) && !value.is_null() {
            object.set(Object::protected_key(name), value.clone());
        }
    }
    if let Some(previous) = args::object(arguments, 5) {
        set_private(object, "previous", Value::Object(previous.clone()));
    }
    Ok(Value::Null)
}

fn get_message(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(protected(this, "message"))
}

/// PDO can store a SQLSTATE string: `$exception->getCode()` must preserve `"42P01"`.
fn get_code(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(protected(this, "code"))
}

fn get_file(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(protected(this, "file"))
}

fn get_line(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(protected(this, "line"))
}

fn get_severity(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(protected(this, "severity"))
}

fn get_trace(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(private(this, "trace"))
}

fn get_previous(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(private(this, "previous"))
}

fn get_trace_as_string(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(Value::from(trace_as_string(&private(this, "trace"))))
}

/// The text PHP prints for an uncaught exception: this exception, preceded
/// by every earlier one in its `previous` chain, oldest first.
fn to_string(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    let mut chain: Vec<Rc<Object>> = Vec::new();
    let mut current = this.cloned();
    while let Some(object) = current {
        // A cycle in the chain would otherwise never end.
        if chain.iter().any(|seen| Rc::ptr_eq(seen, &object)) {
            break;
        }
        current = match private(Some(&object), "previous") {
            Value::Object(previous) => Some(previous),
            _ => None,
        };
        chain.push(object);
    }

    let mut text: Vec<u8> = Vec::new();
    for (position, object) in chain.iter().rev().enumerate() {
        if position > 0 {
            text.extend_from_slice(b"\n\nNext ");
        }
        text.extend_from_slice(&describe(object));
    }
    // PHP keeps the text in the object, where `var_dump` shows it.
    if let Some(object) = this {
        set_private(object, "string", Value::from(text.clone()));
    }
    Ok(Value::from(text))
}

/// `Class: message in file:line`, then the stack trace. PHP leaves out the
/// colon when there is no message.
fn describe(object: &Rc<Object>) -> Vec<u8> {
    let this = Some(object);
    let mut text = object.class.name().as_bytes().to_vec();
    let message = display(&protected(this, "message"));
    if !message.is_empty() {
        text.extend_from_slice(b": ");
        text.extend_from_slice(&message);
    }
    text.extend_from_slice(b" in ");
    text.extend_from_slice(&display(&protected(this, "file")));
    text.push(b':');
    text.extend_from_slice(&display(&protected(this, "line")));
    text.extend_from_slice(b"\nStack trace:\n");
    text.extend_from_slice(&trace_as_string(&private(this, "trace")));
    text
}

/// A scalar as text, without the warnings a real conversion could raise.
fn display(value: &Value) -> Vec<u8> {
    let mut silent = phera_extension_api::value::NoDiagnostics;
    to_php_string(value, &mut silent)
        .map(|text| text.as_bytes().to_vec())
        .unwrap_or_default()
}

/// One line per frame, `#0 file.php(12): name(arguments)`, then `{main}`.
fn trace_as_string(trace: &Value) -> Vec<u8> {
    let empty = Rc::new(PhpArray::new());
    let frames = match trace {
        Value::Array(frames) => frames,
        _ => &empty,
    };
    let mut text = Vec::new();
    for (number, frame) in frames.values().enumerate() {
        text.extend_from_slice(format!("#{number} ").as_bytes());
        if let Value::Array(frame) = &*frame.deref() {
            frame_line(&mut text, frame);
        }
        text.push(b'\n');
    }
    text.extend_from_slice(format!("#{} {{main}}", frames.len()).as_bytes());
    text
}

fn frame_line(text: &mut Vec<u8>, frame: &PhpArray) {
    let field = |name: &str| {
        frame
            .get(&ArrayKey::str(name))
            .map(|value| value.deref().clone())
    };
    match field("file") {
        Some(file) => {
            text.extend_from_slice(&display(&file));
            let line = field("line").unwrap_or(Value::Int(0));
            text.extend_from_slice(
                format!("({}): ", String::from_utf8_lossy(&display(&line))).as_bytes(),
            );
        }
        None => text.extend_from_slice(b"[internal function]: "),
    }
    for part in ["class", "type", "function"] {
        if let Some(value) = field(part) {
            text.extend_from_slice(&display(&value));
        }
    }
    text.push(b'(');
    if let Some(Value::Array(arguments)) = field("args") {
        let rendered: Vec<Vec<u8>> = arguments.values().map(render_argument).collect();
        text.extend_from_slice(&rendered.join(&b", "[..]));
    }
    text.push(b')');
}

/// An argument as a trace shows it: short and without detail, so a trace
/// stays one line per frame and does not leak whole values.
fn render_argument(value: &Value) -> Vec<u8> {
    /// PHP cuts strings in a trace after this many bytes.
    const SHOWN: usize = 15;
    match &*value.deref() {
        Value::Null => b"NULL".to_vec(),
        Value::Bool(flag) => flag.to_string().into_bytes(),
        Value::Int(int) => int.to_string().into_bytes(),
        Value::Float(float) => float_to_string_precision14(*float).into_bytes(),
        Value::String(string) => {
            let bytes = string.as_bytes();
            let mut text = vec![b'\''];
            text.extend_from_slice(bytes.get(..SHOWN).unwrap_or(bytes));
            text.extend_from_slice(if bytes.len() > SHOWN { b"...'" } else { b"'" });
            text
        }
        Value::Array(_) => b"Array".to_vec(),
        Value::Object(object) => format!("Object({})", object.class.name()).into_bytes(),
        Value::Resource(resource) => format!("Resource id #{}", resource.id).into_bytes(),
        Value::Reference(_) => b"NULL".to_vec(),
    }
}
