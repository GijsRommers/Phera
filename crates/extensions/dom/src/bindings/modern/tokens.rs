//! Maintain the ordered, unique tokens in an element's live class attribute.
//!
//! ```php
//! $element->classList->add('selected', 'visible');
//! $element->classList->toggle('selected');
//! ```

mod cursor;
mod offsets;

use std::collections::HashSet;
use std::rc::{Rc, Weak};

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue as C, Context, MethodEntry as M, NativeClone,
    NativeDimensions, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, NativeResult, Object, Param as P, ParamType as T, PhpArray, Registry,
    Value, args,
};

use crate::tree::Name;

use super::super::{Location, This, attributes, dom_error, native, receiver};

const CLASS: &str = "Dom\\TokenList";

#[derive(Clone)]
struct Tokens {
    root: Location,
    _pin: Rc<()>,
}

pub(super) fn wrap(context: &mut dyn Context, root: Location) -> NativeResult {
    if let Some(object) = root
        .shared
        .borrow()
        .class_lists
        .get(&root.id)
        .and_then(Weak::upgrade)
    {
        return Ok(Value::Object(object));
    }
    let value = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &value {
        let pin = {
            let mut state = root.shared.borrow_mut();
            state.class_lists.insert(root.id, Rc::downgrade(object));
            state.pin(root.id)
        };
        *object.native.borrow_mut() = Some(Box::new(Tokens { root, _pin: pin }));
    }
    Ok(value)
}

impl Tokens {
    fn raw(&self) -> String {
        let state = self.root.shared.borrow();
        let arena = &state.document.arena;
        attributes::find(arena, self.root.id, Some(None), "class")
            .map_or_else(String::new, |id| arena[id].value.clone())
    }

    fn write(&self, value: String) -> Result<(), NativeError> {
        let mut state = self.root.shared.borrow_mut();
        let arena = &mut state.document.arena;
        attributes::set(arena, self.root.id, Name::new("class"), value, false)?;
        Ok(())
    }

    fn tokens(&self) -> Vec<String> {
        split_tokens(&self.raw())
    }
}

fn split_tokens(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    raw.split_ascii_whitespace()
        .filter(|&token| seen.insert(token))
        .map(str::to_owned)
        .collect()
}

fn state(object: &Object) -> Result<Tokens, NativeError> {
    native(object).ok_or_else(|| NativeError::error("Dom\\TokenList is uninitialized"))
}

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let state = state(object)?;
    Ok(match name {
        "value" => Some(Value::from(state.raw())),
        "length" => Some(Value::Int(state.tokens().len() as i64)),
        _ => None,
    })
}

fn write(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if name == "value"
        && let NativePropertyMutation::Assign(value) = mutation
    {
        let value = phera_extension_api::value::to_php_string(value, context.diagnostics())?;
        state(object)?.write(value.to_string_lossy())?;
        return Ok(true);
    }
    if matches!(name, "length" | "value") {
        return Err(NativeError::error(format!(
            "Cannot modify Dom\\TokenList::${name}"
        )));
    }
    Ok(false)
}

fn validate(token: &str) -> Result<(), NativeError> {
    if token.is_empty() {
        return Err(dom_error(12, "The empty string is not a valid token"));
    }
    if token.as_bytes().iter().any(u8::is_ascii_whitespace) {
        return Err(dom_error(
            5,
            "The token must not contain any ASCII whitespace",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Operation {
    Add,
    Remove,
    Replace,
}

fn change(this: This<'_>, arguments: &[Value], operation: Operation) -> NativeResult {
    let state = state(receiver(this)?)?;
    let arguments = (0..arguments.len())
        .map(|index| String::from_utf8_lossy(args::bytes(arguments, index)).into_owned())
        .collect::<Vec<_>>();
    for token in &arguments {
        validate(token)?;
    }
    let mut current = state.tokens();
    match operation {
        Operation::Add => {
            let mut unique: HashSet<_> = current.iter().cloned().collect();
            current.extend(
                arguments
                    .into_iter()
                    .filter(|token| unique.insert(token.clone())),
            );
        }
        Operation::Remove => {
            let remove: HashSet<_> = arguments.iter().collect();
            current.retain(|token| !remove.contains(token));
        }
        Operation::Replace => {
            let [before, after] = arguments.as_slice() else {
                return Ok(Value::Bool(false));
            };
            let Some(index) = current.iter().position(|token| token == before) else {
                return Ok(Value::Bool(false));
            };
            if before == after {
                return Ok(Value::Bool(true));
            }
            if current.contains(after) {
                current.remove(index);
            } else {
                current[index] = after.clone();
            }
            state.write(current.join(" "))?;
            return Ok(Value::Bool(true));
        }
    }
    if !current.is_empty() || !state.raw().is_empty() {
        state.write(current.join(" "))?;
    }
    Ok(Value::Null)
}

fn toggle(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let state = state(receiver(this)?)?;
    let token = String::from_utf8_lossy(args::bytes(args, 0));
    validate(&token)?;
    let mut tokens = state.tokens();
    let index = tokens
        .iter()
        .position(|candidate| candidate == token.as_ref());
    let present = args::bool(args, 1).unwrap_or(index.is_none());
    match (present, index) {
        (true, None) => tokens.push(token.into_owned()),
        (false, Some(index)) => {
            tokens.remove(index);
        }
        _ => return Ok(Value::Bool(present)),
    }
    state.write(tokens.join(" "))?;
    Ok(Value::Bool(present))
}

fn item(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let tokens = state(receiver(this)?)?.tokens();
    Ok(args::int(args, 0)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| tokens.get(index))
        .map_or(Value::Null, |token| Value::from(token.as_str())))
}

fn count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    Ok(state(object)?.tokens().len() as i64)
}

pub(super) fn register(registry: &mut Registry) {
    const TOKEN: P = P::new("token", T::String);
    const TOKENS: P = P::new("tokens", T::String).variadic();
    const HOOKS: ClassHooks = ClassHooks {
        clone: Some(NativeClone::Forbidden),
        iterator: Some(cursor::iterator),
        count: Some(count),
        property_access: Some(&NativePropertyAccess {
            read,
            mutate: write,
        }),
        dimensions: Some(NativeDimensions {
            fallback_if_overridden: &[],
            get: offsets::offset,
            exists: |c, o, key| Ok(!offsets::offset(c, o, key)?.is_null()),
            set: |c, o, index, _| offsets::cannot_mutate(c, o, index),
            unset: offsets::cannot_mutate,
            reference: None,
            bind: None,
            direct: None,
        }),
        ..ClassHooks::EMPTY
    };
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        interfaces: &["IteratorAggregate", "Countable"],
        hooks: Some(&HOOKS),
        methods: const {
            &[
                M::public("__construct", &[], super::construct)
                    .with_visibility(phera_extension_api::Visibility::Private),
                M::public("item", const { &[P::new("index", T::Int)] }, item).returning("?string"),
                M::public("contains", &[TOKEN], |_, t, a| {
                    let token = String::from_utf8_lossy(args::bytes(a, 0));
                    Ok(Value::Bool(
                        state(receiver(t)?)?
                            .tokens()
                            .iter()
                            .any(|candidate| candidate == token.as_ref()),
                    ))
                })
                .returning("bool"),
                M::public("add", &[TOKENS], |_, t, a| change(t, a, Operation::Add))
                    .returning("void"),
                M::public("remove", &[TOKENS], |_, t, a| {
                    change(t, a, Operation::Remove)
                })
                .returning("void"),
                M::public(
                    "toggle",
                    const {
                        &[
                            TOKEN,
                            P::new("force", T::Bool).nullable().with_default(C::Null),
                        ]
                    },
                    toggle,
                )
                .returning("bool"),
                M::public(
                    "replace",
                    const { &[TOKEN, P::new("newToken", T::String)] },
                    |_, t, a| change(t, a, Operation::Replace),
                )
                .returning("bool"),
                M::public("supports", &[TOKEN], |_, _, _| {
                    Err(NativeError::type_error(
                        "Attribute \"class\" does not define any supported tokens",
                    ))
                })
                .returning("bool"),
                M::public("count", &[], |c, t, _| {
                    Ok(Value::Int(count(c, receiver(t)?)?))
                })
                .returning("int"),
                M::public("getIterator", &[], |c, t, _| {
                    let iterator = cursor::iterator(c, receiver(t)?.clone())?;
                    c.create_iterator(iterator)
                })
                .returning("Iterator"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
}
