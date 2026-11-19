//! GMP's integer is native state. `serialize(gmp_init(256))` stores `'100'`.

use std::rc::Rc;

use num_bigint::BigInt;
use num_traits::Zero;
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, MethodEntry,
    NativeCastKind, NativeClone, NativeError, NativeResult, Object, Param, ParamType, PhpArray,
    PropertyPurpose, Value, args,
};

use crate::integer::{self, Argument, Result};

mod operators;

pub(crate) const CLASS: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    methods: &[
        MethodEntry::public(
            "__construct",
            &[
                Param::new("num", ParamType::IntOrString).with_default(ConstantValue::Int(0)),
                crate::BASE,
            ],
            construct,
        ),
        MethodEntry::public("__serialize", &[], serialize).returning("array"),
        MethodEntry::public(
            "__unserialize",
            &[Param::new("data", ParamType::Array)],
            unserialize,
        )
        .returning("void"),
    ],
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::copy::<BigInt>()),
        compare: Some(operators::compare_objects),
        binary: Some(operators::binary),
        unary: Some(operators::unary),
        cast: Some(cast),
        properties: Some(properties),
        ..ClassHooks::EMPTY
    }),
    ..ClassEntry::class("GMP")
};

pub(crate) fn value(object: &Object) -> BigInt {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|number| number.downcast_ref())
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn set(object: &Object, number: BigInt) {
    *object.native.borrow_mut() = Some(Box::new(number));
}

pub(crate) fn create(context: &mut dyn Context, number: BigInt) -> NativeResult {
    let result = context.create_object("GMP", PhpArray::new())?;
    if let Value::Object(object) = &result {
        set(object, number);
    }
    Ok(result)
}

pub(crate) fn pair(context: &mut dyn Context, left: BigInt, right: BigInt) -> NativeResult {
    let mut array = PhpArray::new();
    array.insert(ArrayKey::Int(0), create(context, left)?);
    array.insert(ArrayKey::Int(1), create(context, right)?);
    Ok(Value::from(array))
}

fn construct(_: &mut dyn Context, object: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let object = object.ok_or_else(|| NativeError::error("GMP constructor needs an object"))?;
    let number = Argument("GMP::__construct", 1, "num")
        .init(args::value(values, 0), args::int(values, 1).unwrap_or(0))?;
    set(object, number);
    Ok(Value::Null)
}

fn serialize(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let object = object.ok_or_else(|| NativeError::error("GMP serialization needs an object"))?;
    let mut array = PhpArray::new();
    array.insert(
        ArrayKey::Int(0),
        Value::from(value(object).to_str_radix(16)),
    );
    let properties = object.to_array();
    if !properties.is_empty() {
        array.insert(ArrayKey::Int(1), Value::from(properties));
    }
    Ok(Value::from(array))
}

fn unserialize(_: &mut dyn Context, object: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let invalid = || NativeError::throw("Exception", "Could not unserialize number");
    let object = object.ok_or_else(invalid)?;
    let array = args::array(values, 0).ok_or_else(invalid)?;
    let Some(Value::String(number)) = array.get(&ArrayKey::Int(0)) else {
        return Err(invalid());
    };
    let number = integer::parse(number.as_bytes(), 16)
        .ok_or_else(|| NativeError::value_error("Number is not an integer string"))?;
    set(object, number);
    if let Some(properties) = array.get(&ArrayKey::Int(1)) {
        let Value::Array(properties) = properties else {
            return Err(NativeError::throw(
                "Exception",
                "Could not unserialize properties",
            ));
        };
        for (key, value) in properties.iter() {
            let key = match key {
                ArrayKey::Int(index) => ArrayKey::str(index.to_string()),
                key => key,
            };
            object.set(key, value.clone());
        }
    }
    Ok(Value::Null)
}

fn properties(_: &mut dyn Context, object: &Object, purpose: PropertyPurpose) -> Result<PhpArray> {
    let mut properties = object.to_array();
    if purpose == PropertyPurpose::Debug {
        properties.insert(
            ArrayKey::str("num"),
            Value::from(value(object).to_str_radix(10)),
        );
    }
    Ok(properties)
}

fn cast(object: &Object, kind: NativeCastKind) -> Option<Value> {
    let number = value(object);
    Some(match kind {
        NativeCastKind::Bool => Value::Bool(!number.is_zero()),
        NativeCastKind::Int => Value::Int(integer::to_int(&number)),
        NativeCastKind::Float => Value::Float(integer::to_float(&number)),
        NativeCastKind::String => Value::from(number.to_str_radix(10)),
    })
}
