//! `ReflectionClass::newLazyGhost($initializer)` creates a service without calling its constructor.

use std::rc::Rc;

use phera_extension_api::reflection::lazy::{
    LazyClassQuery, LazyPropertyOperation, LazyStrategy, MakeLazy,
};
use phera_extension_api::{
    ConstantValue, Context, LocalFuture, MethodEntry, NativeError, NativeResult, Object, Param,
    ParamType, Value, args,
};

use crate::{class::reflected_class, engine, member::reflected};

pub(crate) const OBJECT: Param = Param::new("object", ParamType::ReflectedObject);
const INITIALIZER: Param = Param::new("initializer", ParamType::Callable);
const FACTORY: Param = Param::new("factory", ParamType::Callable);
const OPTIONS: Param = Param::new("options", ParamType::Int).with_default(ConstantValue::Int(0));

pub(crate) const CLASS_METHODS: &[MethodEntry] = const {
    use LazyClassQuery::*;
    use LazyStrategy::*;
    &[
        MethodEntry::public_async("newLazyGhost", &[INITIALIZER, OPTIONS], |c, o, a| {
            make(c, o, a, Ghost, "newLazyGhost", Shape::New)
        })
        .returning("object"),
        MethodEntry::public_async("newLazyProxy", &[FACTORY, OPTIONS], |c, o, a| {
            make(c, o, a, Proxy, "newLazyProxy", Shape::New)
        })
        .returning("object"),
        MethodEntry::public_async(
            "resetAsLazyGhost",
            &[OBJECT, INITIALIZER, OPTIONS],
            |c, o, a| make(c, o, a, Ghost, "resetAsLazyGhost", Shape::Reset),
        )
        .returning("void"),
        MethodEntry::public_async(
            "resetAsLazyProxy",
            &[OBJECT, FACTORY, OPTIONS],
            |c, o, a| make(c, o, a, Proxy, "resetAsLazyProxy", Shape::Reset),
        )
        .returning("void"),
        MethodEntry::public("initializeLazyObject", &[OBJECT], |c, o, a| {
            query(c, o, a, Initialize, "initializeLazyObject")
        })
        .returning("object"),
        MethodEntry::public("markLazyObjectAsInitialized", &[OBJECT], |c, o, a| {
            query(c, o, a, MarkInitialized, "markLazyObjectAsInitialized")
        })
        .returning("object"),
        MethodEntry::public("isUninitializedLazyObject", &[OBJECT], |c, o, a| {
            query(c, o, a, IsUninitialized, "isUninitializedLazyObject")
        })
        .returning("bool"),
        MethodEntry::public("getLazyInitializer", &[OBJECT], |c, o, a| {
            query(c, o, a, GetInitializer, "getLazyInitializer")
        })
        .returning("?callable"),
    ]
};

enum Shape {
    New,
    Reset,
}

fn make<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    strategy: LazyStrategy,
    method: &'static str,
    shape: Shape,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let reflection = reflected_class(this.as_ref())?;
        let (object, offset) = match shape {
            Shape::New => (None, 0),
            Shape::Reset => (
                Some(receiver(
                    &arguments,
                    &reflection.info.name,
                    "ReflectionClass",
                    method,
                )?),
                1,
            ),
        };
        let initializer = arguments.get(offset).cloned().unwrap_or_default();
        let options = args::int(&arguments, offset + 1).unwrap_or_default();
        engine(context)?
            .reflect_make_lazy(
                &reflection.info,
                MakeLazy {
                    object,
                    initializer,
                    strategy,
                    method,
                    options,
                },
            )
            .await
    })
}

fn query(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
    query: LazyClassQuery,
    method: &str,
) -> NativeResult {
    let reflection = reflected_class(this)?;
    let object = receiver(arguments, &reflection.info.name, "ReflectionClass", method)?;
    engine(context)?.reflect_lazy_object(object, query)
}

pub(crate) fn property(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
    operation: LazyPropertyOperation,
) -> NativeResult {
    let member = reflected(this)?;
    let object = receiver(
        arguments,
        &member.class,
        "ReflectionProperty",
        operation.method(),
    )?;
    engine(context)?.reflect_lazy_property(&member, object, operation)
}

pub(crate) fn write_property(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let member = reflected(this.as_ref())?;
        let object = receiver(
            &arguments,
            &member.class,
            "ReflectionProperty",
            "setRawValueWithoutLazyInitialization",
        )?;
        let value = arguments.get(1).cloned().unwrap_or_default();
        engine(context)?
            .reflect_write_lazy_property(member, object, value)
            .await
    })
}

fn receiver(
    arguments: &[Value],
    class: &str,
    reflector: &str,
    method: &str,
) -> Result<Rc<Object>, NativeError> {
    if let Some(object) = args::object(arguments, 0)
        && object.class.is_instance_of(class)
    {
        return Ok(object.clone());
    }
    let given = arguments
        .first()
        .map_or_else(|| "null".into(), Value::type_name);
    Err(NativeError::type_error(format!(
        "{reflector}::{method}(): Argument #1 ($object) must be of type {class}, {given} given",
    )))
}
