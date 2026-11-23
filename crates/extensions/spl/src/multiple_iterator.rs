//! MultipleIterator reads attached iterators in parallel and retains their labels.
//!
//! ```php
//! $rows = new MultipleIterator(MultipleIterator::MIT_KEYS_ASSOC);
//! $rows->attachIterator($names, 'name');
//! $rows->attachIterator($ids, 'id');
//! ```

use std::rc::Rc;

use indexmap::IndexMap;
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ConstantValue, Context, LocalFuture, MethodEntry as Method,
    NativeClone, NativeComparison, NativeError, NativeResult, Object, Param, ParamType, PhpArray,
    PropertyPurpose, Registry, Value, args,
    value::{Diagnostics, OpError},
};

use crate::{This, read_state, receiver, store, update_state};

const CLASS: &str = "MultipleIterator";

#[derive(Clone)]
struct Entry {
    iterator: Value,
    label: Value,
}

#[derive(Clone)]
struct Multiple {
    flags: i64,
    entries: IndexMap<u32, Entry>,
}

impl Default for Multiple {
    fn default() -> Self {
        Self {
            flags: 1,
            entries: IndexMap::new(),
        }
    }
}

fn read<T>(object: &Object, f: impl FnOnce(&Multiple) -> T) -> Result<T, NativeError> {
    read_state(object, f)
}

fn update<T>(object: &Object, f: impl FnOnce(&mut Multiple) -> T) -> Result<T, NativeError> {
    update_state(object, f)
}

fn iterator(arguments: &[Value]) -> Result<&Rc<Object>, NativeError> {
    args::object(arguments, 0).ok_or_else(|| NativeError::error("Missing sub-iterator"))
}

fn construct(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let flags = args::int(arguments, 0).unwrap_or(1);
    update(object, |state| {
        state.flags = flags;
        Value::Null
    })
}

fn get_flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| Value::Int(state.flags))
}

fn set_flags(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let flags = args::int(arguments, 0).unwrap_or_default();
    update(receiver(this)?, |state| {
        state.flags = flags;
        Value::Null
    })
}

fn attach(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let iterator = iterator(arguments)?;
    let label = args::value(arguments, 1).deref().clone();
    update(receiver(this)?, |state| {
        if !label.is_null() && state.entries.values().any(|entry| entry.label == label) {
            return Err(NativeError::throw(
                "InvalidArgumentException",
                "Key duplication error",
            ));
        }
        if let Some(entry) = state.entries.get_mut(&iterator.id) {
            entry.label = label;
        } else {
            state.entries.insert(
                iterator.id,
                Entry {
                    iterator: Value::Object(iterator.clone()),
                    label,
                },
            );
        }
        Ok(Value::Null)
    })?
}

fn detach(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let id = iterator(arguments)?.id;
    update(receiver(this)?, |state| {
        state.entries.shift_remove(&id);
        Value::Null
    })
}

fn contains(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let id = iterator(arguments)?.id;
    read(receiver(this)?, |state| {
        Value::Bool(state.entries.contains_key(&id))
    })
}

fn count(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        Value::Int(state.entries.len() as i64)
    })
}

#[derive(Clone, Copy)]
enum Operation {
    Rewind,
    Next,
    Valid,
    Current,
    Key,
}

fn run<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    operation: Operation,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (flags, entries) = read(object, |state| {
            (
                state.flags,
                state.entries.values().cloned().collect::<Vec<_>>(),
            )
        })?;
        if entries.is_empty() {
            return match operation {
                Operation::Current => Err(runtime("Called current() on an invalid iterator")),
                Operation::Key => Err(runtime("Called key() on an invalid iterator")),
                Operation::Valid => Ok(Value::Bool(false)),
                _ => Ok(Value::Null),
            };
        }
        let need_all = flags & 1 != 0;
        let mut output = PhpArray::new();
        for entry in entries {
            let Value::Object(iterator) = &entry.iterator else {
                continue;
            };
            if Rc::ptr_eq(iterator, object) {
                return Err(runtime(
                    "Cannot iterate a MultipleIterator attached to itself",
                ));
            }
            match operation {
                Operation::Rewind | Operation::Next => {
                    let method = if matches!(operation, Operation::Rewind) {
                        "rewind"
                    } else {
                        "next"
                    };
                    context
                        .call_method(iterator.clone(), method, Vec::new())
                        .await?;
                }
                Operation::Valid => {
                    let valid = is_valid(context, iterator.clone()).await?;
                    if need_all != valid {
                        return Ok(Value::Bool(valid));
                    }
                }
                Operation::Current | Operation::Key => {
                    let method = if matches!(operation, Operation::Current) {
                        "current"
                    } else {
                        "key"
                    };
                    let value = if is_valid(context, iterator.clone()).await? {
                        context
                            .call_method(iterator.clone(), method, Vec::new())
                            .await?
                    } else if need_all {
                        return Err(runtime(format!(
                            "Called {method}() with non valid sub iterator"
                        )));
                    } else {
                        Value::Null
                    };
                    if flags & 2 != 0 {
                        if entry.label.is_null() {
                            return Err(NativeError::throw(
                                "InvalidArgumentException",
                                "Sub-Iterator is associated with NULL",
                            ));
                        }
                        let key = ArrayKey::from_value(&entry.label)?;
                        output.insert(key, value);
                    } else {
                        output.push(value)?;
                    }
                }
            }
        }
        Ok(match operation {
            Operation::Current | Operation::Key => Value::from(output),
            Operation::Valid => Value::Bool(need_all),
            _ => Value::Null,
        })
    })
}

async fn is_valid(context: &mut dyn Context, iterator: Rc<Object>) -> Result<bool, NativeError> {
    let result = context.call_method(iterator, "valid", Vec::new()).await?;
    Ok(matches!(&*result.deref(), Value::Bool(true)))
}

fn runtime(message: impl Into<String>) -> NativeError {
    NativeError::throw("RuntimeException", message)
}

fn clone_iterators(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    store(copy, read(original, Clone::clone)?)
}

fn compare(_: &Object, _: &Object, _: &mut dyn Diagnostics) -> Result<NativeComparison, OpError> {
    Ok(NativeComparison::Uncomparable)
}

fn view(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut props = object.to_array();
    if purpose == PropertyPurpose::Debug {
        let entries = read(object, |state| {
            crate::list(state.entries.values().map(|entry| {
                Value::from(
                    [
                        (ArrayKey::str("obj"), entry.iterator.clone()),
                        (ArrayKey::str("inf"), entry.label.clone()),
                    ]
                    .into_iter()
                    .collect::<PhpArray>(),
                )
            }))
        })?;
        props.insert(Object::private_key("SplObjectStorage", "storage"), entries);
    }
    Ok(props)
}

fn debug(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    view(context, receiver(this)?, PropertyPurpose::Debug).map(Value::from)
}

const ITERATOR: Param = Param::new("iterator", ParamType::Class("Iterator"));
const FLAGS: Param = Param::new("flags", ParamType::Int);
const INFO: Param = Param::new("info", ParamType::IntOrString)
    .nullable()
    .with_default(ConstantValue::Null);

const HOOKS: ClassHooks = ClassHooks {
    fork: Some(crate::native_state::fork_spl),
    clone: Some(NativeClone::Copy(clone_iterators)),
    compare: Some(compare),
    properties: Some(view),
    ..ClassHooks::EMPTY
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Iterator"],
        hooks: Some(&HOOKS),
        constants: &[
            ("MIT_NEED_ALL", ConstantValue::Int(1)),
            ("MIT_NEED_ANY", ConstantValue::Int(0)),
            ("MIT_KEYS_NUMERIC", ConstantValue::Int(0)),
            ("MIT_KEYS_ASSOC", ConstantValue::Int(2)),
        ],
        methods: const {
            &[
                Method::public(
                    "__construct",
                    const { &[FLAGS.with_default(ConstantValue::Int(1))] },
                    construct,
                ),
                Method::public("getFlags", &[], get_flags).tentative_return("int"),
                Method::public("setFlags", &[FLAGS], set_flags).tentative_return("void"),
                Method::public("attachIterator", &[ITERATOR, INFO], attach)
                    .tentative_return("void"),
                Method::public("detachIterator", &[ITERATOR], detach).tentative_return("void"),
                Method::public("containsIterator", &[ITERATOR], contains).tentative_return("bool"),
                Method::public("countIterators", &[], count).tentative_return("int"),
                Method::public_async("rewind", &[], |c, o, _| run(c, o, Operation::Rewind))
                    .tentative_return("void"),
                Method::public_async("next", &[], |c, o, _| run(c, o, Operation::Next))
                    .tentative_return("void"),
                Method::public_async("valid", &[], |c, o, _| run(c, o, Operation::Valid))
                    .tentative_return("bool"),
                Method::public_async("key", &[], |c, o, _| run(c, o, Operation::Key))
                    .tentative_return("array"),
                Method::public_async("current", &[], |c, o, _| run(c, o, Operation::Current))
                    .tentative_return("array"),
                Method::public("__debugInfo", &[], debug).tentative_return("array"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
}

impl crate::native_state::ForkValues for Multiple {
    fn fork_values(
        &mut self,
        fork: &mut phera_extension_api::value::Fork<'_>,
    ) -> Result<(), String> {
        for entry in self.entries.values_mut() {
            crate::native_state::fork_value(&mut entry.iterator, fork);
            crate::native_state::fork_value(&mut entry.label, fork);
        }
        Ok(())
    }
}

pub(crate) const FORK: crate::native_state::StoredFork = crate::native_state::fork_as::<Multiple>;
