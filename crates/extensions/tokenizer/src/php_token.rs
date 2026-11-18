//! `(new PhpToken(T_STRING, "name"))->is([T_STRING, "other"])` returns true.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ConstantValue, Context, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, PropertyEntry, Registry, Value, Visibility, args,
};

use crate::{names, stream, token};

const CLASS: &str = "PhpToken";
const PROPERTIES: &[PropertyEntry] = &[
    PropertyEntry::new("id", ConstantValue::Null, Visibility::Public),
    PropertyEntry::new("text", ConstantValue::Null, Visibility::Public),
    PropertyEntry::new("line", ConstantValue::Null, Visibility::Public),
    PropertyEntry::new("pos", ConstantValue::Null, Visibility::Public),
];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("tokenize", stream::PARAMETERS, tokenize).static_(),
    MethodEntry::public(
        "__construct",
        &[
            Param::new("id", ParamType::Int),
            Param::new("text", ParamType::String),
            Param::new("line", ParamType::Int).optional(),
            Param::new("pos", ParamType::Int).optional(),
        ],
        construct,
    )
    .final_(),
    MethodEntry::public("is", &[Param::new("kind", ParamType::Mixed)], is),
    MethodEntry::public("isIgnorable", &[], is_ignorable),
    MethodEntry::public("getTokenName", &[], get_token_name),
    MethodEntry::public("__toString", &[], to_string),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Stringable"],
        properties: PROPERTIES,
        methods: METHODS,
        ..ClassEntry::class(CLASS)
    });
}

fn tokenize(context: &mut dyn Context, _: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    stream::tokenize(
        context,
        args::bytes(args, 0),
        args::int(args, 1).unwrap_or_default(),
        |context, token| {
            context.create_object(
                CLASS,
                properties(
                    token.id,
                    Value::from(token.text),
                    token.line,
                    token.pos as i64,
                ),
            )
        },
    )
}

fn properties(id: i64, text: Value, line: i64, pos: i64) -> PhpArray {
    let mut properties = PhpArray::new();
    for (name, value) in [
        ("id", Value::Int(id)),
        ("text", text),
        ("line", Value::Int(line)),
        ("pos", Value::Int(pos)),
    ] {
        properties.insert(ArrayKey::str(name), value);
    }
    properties
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let this = this.expect("instance method");
    let values = properties(
        args::int(args, 0).expect("required integer"),
        args::value(args, 1).clone(),
        args::int(args, 2).unwrap_or(-1),
        args::int(args, 3).unwrap_or(-1),
    );
    this.update(|properties| {
        for (key, value) in values.iter() {
            properties.insert(key, value.clone());
        }
    });
    Ok(Value::Null)
}

fn property(this: &Object, name: &str) -> NativeResult {
    this.get(&ArrayKey::str(name))
        .filter(|value| !matches!(*value.deref(), Value::Null))
        .ok_or_else(|| {
            NativeError::error(format!(
                "Typed property PhpToken::${name} must not be accessed before initialization"
            ))
        })
}

fn id(this: &Object) -> Result<i64, NativeError> {
    match *property(this, "id")?.deref() {
        Value::Int(id) => Ok(id),
        _ => Err(NativeError::type_error("PhpToken::$id must hold an int")),
    }
}

fn to_string(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    property(this.expect("instance method"), "text").map(|value| value.deref().clone())
}

fn get_token_name(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let id = id(this.expect("instance method"))?;
    Ok(if id < 256 {
        Value::from(&[id as u8][..])
    } else {
        names::name(id).map_or(Value::Null, Value::from)
    })
}

fn is_ignorable(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(token::ignorable(id(
        this.expect("instance method")
    )?)))
}

fn is(_: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let this = this.expect("instance method");
    let kind = args::value(args, 0).deref();
    if let Value::Array(kinds) = &*kind {
        for kind in kinds.values() {
            if matches_kind(this, &kind.deref(), true)? {
                return Ok(Value::Bool(true));
            }
        }
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(matches_kind(this, &kind, false)?))
}

fn matches_kind(this: &Object, kind: &Value, array_entry: bool) -> Result<bool, NativeError> {
    match kind {
        Value::Int(kind) => Ok(id(this)? == *kind),
        Value::String(kind) => {
            let text = property(this, "text")?;
            Ok(matches!(&*text.deref(), Value::String(text) if text == kind))
        }
        _ => {
            let requirement = if array_entry {
                "must only have elements of type string|int"
            } else {
                "must be of type string|int|array"
            };
            let given = match kind {
                Value::Bool(true) => "true".to_string(),
                Value::Bool(false) => "false".to_string(),
                other => other.type_name(),
            };
            Err(NativeError::type_error(format!(
                "PhpToken::is(): Argument #1 ($kind) {requirement}, {given} given"
            )))
        }
    }
}
