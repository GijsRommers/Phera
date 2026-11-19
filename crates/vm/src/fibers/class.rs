//! `Fiber::getCurrent()` returns null on the main stack. `new Fiber($callback)`
//! prepares a callback and leaves it in the unstarted state.

use std::rc::Rc;

use phera_extension_api::fiber::{FiberOperation, fiber_error};
use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry,
    NativeClone, NativeError, NativeResult, Param, ParamType,
};
use phera_value::{ArrayKey, Object, PhpArray, PhpString, Value};

use super::{State, with_state};

const CALLBACK: &[Param] = &[Param::new("callback", ParamType::Callable)];
const ARGUMENTS: &[Param] = &[Param::new("args", ParamType::Mixed).variadic()];
const VALUE: &[Param] = &[Param::new("value", ParamType::Mixed).with_default(ConstantValue::Null)];
const EXCEPTION: &[Param] = &[Param::new("exception", ParamType::Class("Throwable"))];

pub(super) const FIBER: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Forbidden),
        ..ClassHooks::EMPTY
    }),
    methods: &[
        MethodEntry::public_async("__construct", CALLBACK, construct),
        MethodEntry::public_async_named("start", ARGUMENTS, start).returning("mixed"),
        MethodEntry::public_async("resume", VALUE, resume).returning("mixed"),
        MethodEntry::public_async("throw", EXCEPTION, throw).returning("mixed"),
        MethodEntry::public("isStarted", &[], |_, object, _| {
            status(object, |state| !matches!(state, State::Ready(_)))
        })
        .returning("bool"),
        MethodEntry::public("isSuspended", &[], |_, object, _| {
            status(object, |state| matches!(state, State::Suspended(_)))
        })
        .returning("bool"),
        MethodEntry::public("isRunning", &[], |_, object, _| {
            status(object, |state| matches!(state, State::Running))
        })
        .returning("bool"),
        MethodEntry::public("isTerminated", &[], |_, object, _| {
            status(object, |state| {
                matches!(state, State::Returned(_) | State::Failed | State::Exited)
            })
        })
        .returning("bool"),
        MethodEntry::public("getReturn", &[], get_return).returning("mixed"),
        MethodEntry::public("getCurrent", &[], get_current)
            .static_()
            .returning("?Fiber"),
        MethodEntry::public_async("suspend", VALUE, suspend)
            .static_()
            .returning("mixed"),
    ],
    ..ClassEntry::class("Fiber")
};

pub(super) const FIBER_ERROR: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    parent: Some("Error"),
    methods: &[MethodEntry::public("__construct", &[], reserved)],
    ..ClassEntry::class("FiberError")
};

fn reserved(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "The \"FiberError\" class is reserved for internal use and cannot be manually instantiated",
    ))
}

fn construct(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    mut values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operate(
        context,
        object,
        FiberOperation::Construct(values.pop().unwrap_or_default()),
    )
}

fn start<'a>(
    context: &'a mut dyn Context,
    object: Option<Rc<Object>>,
    arguments: Vec<Value>,
    names: &'a [(usize, PhpString)],
) -> LocalFuture<'a, NativeResult> {
    let mut args = PhpArray::new();
    let mut names = names.iter().peekable();
    for (index, value) in arguments.into_iter().enumerate() {
        if let Some((position, name)) = names.peek()
            && *position == index
        {
            args.insert(ArrayKey::str(name.clone()), value);
            names.next();
        } else if let Err(error) = args.push(value) {
            return Box::pin(std::future::ready(Err(error.into())));
        }
    }
    operate(context, object, FiberOperation::Start(args))
}

fn resume(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    mut args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operate(context, object, FiberOperation::Resume(args.pop()))
}

fn throw(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    mut args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operate(
        context,
        object,
        FiberOperation::Throw(args.pop().unwrap_or_default()),
    )
}

fn suspend(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    mut args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    context.suspend_fiber(args.pop())
}

fn operate(
    context: &mut dyn Context,
    object: Option<Rc<Object>>,
    operation: FiberOperation,
) -> LocalFuture<'_, NativeResult> {
    match object {
        Some(object) => context.fiber_operation(object, operation),
        None => Box::pin(std::future::ready(Err(internal_error!(
            "fiber method receiver"
        )))),
    }
}

fn get_current(context: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(context.current_fiber().map_or(Value::Null, Value::Object))
}

fn status(object: Option<&Rc<Object>>, test: impl FnOnce(&State) -> bool) -> NativeResult {
    let object = object.ok_or_else(|| internal_error!("fiber status receiver"))?;
    Ok(Value::Bool(with_state(object, |fiber| test(&fiber.state))?))
}

fn get_return(context: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let object = object.ok_or_else(|| internal_error!("fiber return receiver"))?;
    with_state(object, |fiber| {
        let message = match &fiber.state {
            State::Ready(_) => "The fiber has not been started",
            State::Running | State::Suspended(_) => "The fiber has not returned",
            State::Failed => "The fiber threw an exception",
            State::Exited => "The fiber exited with a fatal error",
            State::Returned(value) => return Ok(value.deref().clone()),
        };
        Err(fiber_error(format!(
            "Cannot get fiber return value: {message}"
        )))
    })?
    .map_err(|error| context.native_call_error("Fiber::getReturn", &[], error))
}
