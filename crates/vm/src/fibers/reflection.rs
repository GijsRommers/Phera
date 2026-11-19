//! `(new ReflectionFiber($fiber))->getExecutingLine()` inspects the parked
//! interpreter without running the remaining PHP instructions.

use std::rc::Rc;

use phera_extension_api::fiber::FiberInspection;
use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry,
    NativeClone, NativeError, NativeResult, Param, ParamType, args,
};
use phera_value::{Object, Value};

use super::{State, with_state};

const FIBER: &[Param] = &[Param::new("fiber", ParamType::Class("Fiber"))];
const TRACE: &[Param] =
    &[Param::new("options", ParamType::Int).with_default(ConstantValue::Int(1))];

pub(super) const CLASS: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Forbidden),
        ..ClassHooks::EMPTY
    }),
    methods: &[
        MethodEntry::public("__construct", FIBER, construct),
        MethodEntry::public("getFiber", &[], |_, object, _| {
            Ok(Value::Object(target(object)?))
        })
        .returning("Fiber"),
        MethodEntry::public("getCallable", &[], callable).returning("callable"),
        MethodEntry::public_async("getExecutingFile", &[], |cx, object, _| {
            inspect(cx, object.as_ref(), FiberInspection::File)
        })
        .returning("?string"),
        MethodEntry::public_async("getExecutingLine", &[], |cx, object, _| {
            inspect(cx, object.as_ref(), FiberInspection::Line)
        })
        .returning("?int"),
        MethodEntry::public_async("getTrace", TRACE, |cx, object, args| {
            inspect(
                cx,
                object.as_ref(),
                FiberInspection::Trace(args::int(&args, 0).unwrap_or(1)),
            )
        })
        .returning("array"),
    ],
    ..ClassEntry::class("ReflectionFiber")
};

pub(super) fn invalid_state() -> NativeError {
    NativeError::error(
        "Cannot fetch information from a fiber that has not been started or is terminated",
    )
}

fn construct(
    _: &mut dyn Context,
    object: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let object = object.ok_or_else(|| internal_error!("fiber reflection receiver"))?;
    let fiber =
        args::object(arguments, 0).ok_or_else(|| internal_error!("fiber reflection argument"))?;
    *object
        .native
        .try_borrow_mut()
        .map_err(|_| internal_error!("fiber reflection state borrowed"))? =
        Some(Box::new(fiber.clone()));
    Ok(Value::Null)
}

fn target(object: Option<&Rc<Object>>) -> Result<Rc<Object>, NativeError> {
    let object = object.ok_or_else(|| internal_error!("fiber reflection receiver"))?;
    object
        .native
        .try_borrow()
        .map_err(|_| internal_error!("fiber reflection state borrowed"))?
        .as_ref()
        .and_then(|state| state.downcast_ref::<Rc<Object>>())
        .cloned()
        .ok_or_else(|| NativeError::error("ReflectionFiber has not been initialized"))
}

fn callable(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let target = target(object)?;
    with_state(&target, |fiber| {
        if matches!(
            fiber.state,
            State::Returned(_) | State::Exited | State::Failed
        ) {
            Err(NativeError::error(
                "Cannot fetch the callable from a fiber that has terminated",
            ))
        } else {
            Ok(fiber.callable.clone())
        }
    })?
}

fn inspect<'a>(
    context: &'a mut dyn Context,
    object: Option<&Rc<Object>>,
    inspection: FiberInspection,
) -> LocalFuture<'a, NativeResult> {
    match target(object) {
        Ok(target) => context.inspect_fiber(target, inspection),
        Err(error) => Box::pin(std::future::ready(Err(error))),
    }
}
