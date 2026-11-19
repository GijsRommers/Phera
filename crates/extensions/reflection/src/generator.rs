//! `new ReflectionGenerator($iterator)` inspects a generator without resuming it.

use std::rc::{Rc, Weak};

use phera_extension_api::reflection::GeneratorQuery;
use phera_extension_api::{
    ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, Registry, Value,
};

use crate::{UNCLONEABLE, bind, engine, keep, load, receiver};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: const {
            &[
                MethodEntry::public(
                    "__construct",
                    const { &[Param::new("generator", ParamType::Class("Generator"))] },
                    construct,
                ),
                MethodEntry::public("getExecutingLine", &[], |cx, this, _| {
                    inspect(cx, this, GeneratorQuery::Line)
                })
                .returning("int"),
                MethodEntry::public("getExecutingFile", &[], |cx, this, _| {
                    inspect(cx, this, GeneratorQuery::File)
                })
                .returning("string"),
                MethodEntry::public(
                    "getTrace",
                    const {
                        &[
                            Param::new("options", ParamType::Int)
                                .with_default(ConstantValue::Int(1)),
                        ]
                    },
                    |cx, this, arguments| {
                        let options = phera_extension_api::args::int(arguments, 0).unwrap_or(1);
                        inspect(cx, this, GeneratorQuery::Trace(options))
                    },
                )
                .returning("array"),
                MethodEntry::public("getFunction", &[], |cx, this, _| {
                    let generator = generator(this)?;
                    let target = engine(cx)?.reflect_generator_function(&generator)?;
                    crate::function::make(cx, target)
                })
                .returning("ReflectionFunctionAbstract"),
                MethodEntry::public("getThis", &[], |cx, this, _| {
                    inspect(cx, this, GeneratorQuery::This)
                })
                .returning("?object"),
                MethodEntry::public("getExecutingGenerator", &[], |cx, this, _| {
                    inspect(cx, this, GeneratorQuery::Executing)
                })
                .returning("Generator"),
                MethodEntry::public("isClosed", &[], |cx, this, _| {
                    inspect(cx, this, GeneratorQuery::Closed)
                })
                .returning("bool"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionGenerator")
    });
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let Some(Value::Object(generator)) = arguments.first() else {
        return Err(NativeError::error(
            "ReflectionGenerator requires a Generator",
        ));
    };
    bind(object, None, Rc::downgrade(generator))?;
    keep(object, Value::Object(generator.clone()));
    Ok(Value::Null)
}

fn generator(this: Option<&Rc<Object>>) -> Result<Rc<Object>, NativeError> {
    load::<Weak<Object>>(this)?
        .upgrade()
        .ok_or_else(|| NativeError::error("Generator is unavailable"))
}

fn inspect(cx: &mut dyn Context, this: Option<&Rc<Object>>, query: GeneratorQuery) -> NativeResult {
    engine(cx)?.reflect_generator(&generator(this)?, query)
}
