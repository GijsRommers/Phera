//! Iterate over native intl enumeration and break part results.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ConstantValue, Context, MethodEntry, NativeError, NativeResult, Object, PhpArray,
    Registry, Value,
};

const CLASS: &str = "IntlIterator";
const PARTS: &str = "IntlPartsIterator";

pub type Entry = (Value, Value, i64);

struct State {
    entries: Vec<Entry>,
    index: usize,
    source: Option<Rc<Object>>,
}

pub fn values(context: &mut dyn Context, entries: impl Iterator<Item = Value>) -> NativeResult {
    let entries = entries
        .enumerate()
        .map(|(index, value)| (Value::Int(index as i64), value, 0))
        .collect();
    make(context, CLASS, entries, None)
}

pub fn parts(context: &mut dyn Context, entries: Vec<Entry>, source: Rc<Object>) -> NativeResult {
    make(context, PARTS, entries, Some(source))
}

fn make(
    context: &mut dyn Context,
    class: &str,
    entries: Vec<Entry>,
    source: Option<Rc<Object>>,
) -> NativeResult {
    let result = context.create_object(class, PhpArray::new())?;
    if let Value::Object(object) = &result {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(State {
            entries,
            index: 0,
            source,
        }));
    }
    Ok(result)
}

fn apply(object: Option<&Rc<Object>>, action: impl FnOnce(&mut State) -> Value) -> NativeResult {
    let mut payload = object
        .ok_or_else(missing)?
        .native_payload_mut()
        .map_err(|_| missing())?;
    payload
        .as_deref_mut()
        .and_then(|value| value.downcast_mut::<State>())
        .map(action)
        .ok_or_else(missing)
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed IntlIterator")
}

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Iterator"],
        methods: const {
            &[
                MethodEntry::public("current", &[], |_, object, _| {
                    apply(object, |state| {
                        state
                            .entries
                            .get(state.index)
                            .map_or(Value::Null, |entry| entry.1.clone())
                    })
                })
                .returning("mixed"),
                MethodEntry::public("key", &[], |_, object, _| {
                    apply(object, |state| {
                        state
                            .entries
                            .get(state.index)
                            .map_or(Value::Null, |entry| entry.0.clone())
                    })
                })
                .returning("mixed"),
                MethodEntry::public("next", &[], |_, object, _| {
                    apply(object, |state| {
                        state.index = state.index.saturating_add(1);
                        Value::Null
                    })
                })
                .returning("void"),
                MethodEntry::public("rewind", &[], |_, object, _| {
                    apply(object, |state| {
                        state.index = 0;
                        Value::Null
                    })
                })
                .returning("void"),
                MethodEntry::public("valid", &[], |_, object, _| {
                    apply(object, |state| {
                        Value::Bool(state.index < state.entries.len())
                    })
                })
                .returning("bool"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        constants: &[
            ("KEY_SEQUENTIAL", ConstantValue::Int(0)),
            ("KEY_LEFT", ConstantValue::Int(1)),
            ("KEY_RIGHT", ConstantValue::Int(2)),
        ],
        methods: const {
            &[
                MethodEntry::public("getBreakIterator", &[], |_, object, _| {
                    apply(object, |state| {
                        state.source.clone().map_or(Value::Null, Value::Object)
                    })
                })
                .returning("IntlBreakIterator"),
                MethodEntry::public("getRuleStatus", &[], |_, object, _| {
                    apply(object, |state| {
                        let index = state.index.min(state.entries.len().saturating_sub(1));
                        Value::Int(state.entries.get(index).map_or(0, |entry| entry.2))
                    })
                })
                .returning("int"),
            ]
        },
        ..ClassEntry::class(PARTS)
    });
}
