//! ArrayObject exchanges and serializes storage without changing its PHP identity.
//!
//! ```php
//! $a = new ArrayObject([1]);
//! $old = $a->exchangeArray([2]); // [1]
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, MethodPreparation, NativeError, NativeResult, Object, Param, PhpString,
    Value, args,
};

use super::{
    ITERATOR,
    dimensions::{self, arg},
    properties,
    state::{self, Flags, Position, State, Storage, Target},
};
use crate::{This, receiver};

pub(super) fn construct_iterator(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    replace_storage(context, receiver(this)?, arguments, "__construct")
}

pub(super) fn construct_object(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        if let Some(name) = args::string(&arguments, 2) {
            let class = resolve_iterator(context, name, "__construct", 3).await?;
            state::update(object, |state| {
                state.iterator_class = class;
                Ok(())
            })?;
        }
        replace_storage(context, object, &arguments, "__construct")
    })
}

pub(super) fn storage(
    context: &mut dyn Context,
    receiver: &Object,
    value: &Value,
    method: &str,
) -> Result<Storage, NativeError> {
    match &*value.deref() {
        Value::Array(array) => Ok(Storage::Array(array.clone())),
        Value::Object(object) => {
            let base = state::base(receiver);
            context.diagnostics().deprecated(format!(
                "{base}::{method}(): Using an object as a backing array for {base} is deprecated, \
                 as it allows violating class constraints and invariants"
            ));
            if std::ptr::eq(receiver, object.as_ref()) {
                Ok(Storage::OwnProperties)
            } else if object.class.is_instance_of(super::OBJECT)
                || object.class.is_instance_of(ITERATOR)
            {
                Ok(Storage::Alias(object.clone()))
            } else if object.class.has_native_properties() {
                Err(NativeError::throw(
                    "InvalidArgumentException",
                    format!(
                        "Overloaded object of type {} is not compatible with {}",
                        object.class.name(),
                        receiver.class.name(),
                    ),
                ))
            } else {
                Ok(Storage::Properties(object.clone()))
            }
        }
        _ => Err(NativeError::throw(
            "InvalidArgumentException",
            "Passed variable is not an array or object",
        )),
    }
}

fn replace_storage(
    context: &mut dyn Context,
    object: &Object,
    arguments: &[Value],
    method: &str,
) -> NativeResult {
    let Some(value) = arguments.first() else {
        return Ok(Value::Null);
    };
    state::check_mutable(object)?;
    let storage = storage(context, object, value, method)?;
    let flags = match &storage {
        Storage::Alias(alias) if arguments.len() == 1 => Some(state::read(alias, |s| s.flags)?),
        _ => args::int(arguments, 1).map(Flags::new),
    };
    state::update(object, |state| {
        state.storage = storage;
        state.position = Position::Initial;
        if let Some(flags) = flags {
            state.flags = Flags::new(state.flags.raw() | flags.raw());
        }
        Ok(Value::Null)
    })
}

pub(super) fn append(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        if Target::of(object)?.is_properties() {
            return Err(NativeError::error(format!(
                "Cannot append properties to objects, use {}::offsetSet() instead",
                object.class.name()
            )));
        }
        let value = arg(&arguments, 0);
        if context.is_user_method(object, "offsetSet") {
            context
                .call_method(object.clone(), "offsetSet", vec![Value::Null, value])
                .await?;
        } else {
            dimensions::set(context, object, Value::Null, value)?;
        }
        Ok(Value::Null)
    })
}

pub(super) fn get_array_copy(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Array(Target::of(receiver(this)?)?.copy()?))
}

pub(super) fn exchange_array(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let object = receiver(this)?;
    state::check_mutable(object)?;
    let previous = Target::of(object)?.copy()?;
    replace_storage(context, object, arguments, "exchangeArray")?;
    Ok(Value::Array(previous))
}

pub(super) fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    stored_count(object)
}

pub(super) fn direct_count(object: &Object, _: &[Option<Value>]) -> Option<Value> {
    stored_count(object).ok().map(Value::Int)
}

fn stored_count(object: &Object) -> Result<i64, NativeError> {
    let target = Target::of(object)?;
    target.read(|array| {
        if target.is_properties() {
            array.keys().filter(state::public).count() as i64
        } else {
            array.len() as i64
        }
    })
}

pub(super) fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(native_count(context, receiver(this)?)?))
}

pub(super) fn get_flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(state::read(receiver(this)?, |state| {
        state.flags.raw()
    })?))
}

pub(super) fn set_flags(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    state::update(receiver(this)?, |state| {
        state.flags = Flags::new(args::int(args, 0).unwrap_or_default());
        Ok(Value::Null)
    })
}

pub(super) fn get_iterator(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let (flags, iterator_class) = state::read(object, |s| (s.flags, s.iterator_class.clone()))?;
    let class = iterator_class.as_deref().unwrap_or(ITERATOR);
    let iterator = crate::make(
        context,
        class,
        State {
            flags,
            storage: Storage::Alias(object.clone()),
            iterator_class: iterator_class.clone(),
            ..State::default()
        },
    )?;
    Ok(Value::Object(iterator))
}

pub(super) fn set_iterator_class(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::string(&arguments, 0)
            .ok_or_else(|| NativeError::error("Missing iterator class name"))?;
        let class = resolve_iterator(context, name, "setIteratorClass", 1).await?;
        state::update(receiver(this.as_ref())?, |state| {
            state.iterator_class = class;
            Ok(Value::Null)
        })
    })
}

async fn resolve_iterator(
    context: &mut dyn Context,
    name: &PhpString,
    method: &str,
    position: usize,
) -> Result<Option<String>, NativeError> {
    let name = name.to_string_lossy();
    let class = context.lookup_class(&name).await?;
    let Some(class) = class.filter(|class| class.is_instance_of(ITERATOR)) else {
        return Err(NativeError::type_error(format!(
            "ArrayObject::{method}(): Argument #{position} ($iteratorClass) must be a class \
             name derived from ArrayIterator, {name} given"
        )));
    };
    Ok((class.name() != ITERATOR).then(|| class.name().to_owned()))
}

pub(super) fn get_iterator_class(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    state::read(receiver(this)?, |state| {
        Value::from(state.iterator_class.as_deref().unwrap_or(ITERATOR))
    })
}

pub(super) fn debug(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::from(properties::debug(receiver(this)?)?))
}

pub(super) fn prepare_iterator_class(
    context: &mut dyn Context,
    _: &[Param],
    args: &mut [Value],
) -> Result<MethodPreparation, NativeError> {
    if let Some(Value::Array(_)) = args.first()
        && let Some(value) = args.first_mut()
    {
        *value = Value::String(phera_extension_api::value::to_php_string(
            value,
            context.diagnostics(),
        )?);
    }
    Ok(MethodPreparation::Default)
}
