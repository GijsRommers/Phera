//! Hashing in pieces through a `HashContext`:
//!
//! ```php
//! $h = hash_init('sha256');
//! hash_update($h, 'hel');
//! $copy = hash_copy($h);
//! hash_update($h, 'lo');
//! hash_final($h);       // hash('sha256', 'hello')
//! hash_final($copy);    // hash('sha256', 'hel')
//! hash_update($h, 'x'); // TypeError: the context is finalized
//! ```

use std::any::Any;
use std::rc::Rc;

use digest::DynDigest;
use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, FunctionEntry, MethodEntry, NativeClone,
    NativeError, NativeResult, Object, Param, ParamType, PhpArray, Value, Visibility, args,
};

use crate::algorithms::Algorithm;
use crate::digests::output;
use crate::given;
use crate::hmac::Hmac;
use crate::options::{self, Options};

pub const FUNCTIONS: &[FunctionEntry] = &[
    HASH_INIT,
    HASH_UPDATE,
    HASH_FINAL,
    HASH_COPY,
    HASH_UPDATE_STREAM,
];

/// The flag of `hash_init()` that asks for HMAC.
pub const HASH_HMAC: i64 = 1;

/// `final class HashContext`, made only by `hash_init()` and `hash_copy()`.
pub const HASH_CONTEXT: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Copy(clone_hash_context)),
        ..ClassHooks::EMPTY
    }),
    methods: &[
        MethodEntry::public("__construct", &[], construct).with_visibility(Visibility::Private)
    ],
    ..ClassEntry::class("HashContext")
};

fn clone_hash_context(source: &Rc<Object>, destination: &Object) -> Result<(), NativeError> {
    let state = source
        .native
        .borrow()
        .as_ref()
        .and_then(|state| state.downcast_ref::<State>())
        .cloned()
        .ok_or_else(|| NativeError::value_error("Cannot clone a finalized HashContext"))?;
    put_state(destination, state);
    Ok(())
}

fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct HashContext, use hash_init() instead",
    ))
}

/// What a `HashContext` holds between `hash_init()` and `hash_final()`.
#[derive(Clone)]
pub struct State {
    algorithm: Algorithm,
    options: Options,
    input: Input,
    /// Set by `hash_init()` with `HASH_HMAC`.
    hmac: Option<Hmac>,
}

/// The input so far.
#[derive(Clone)]
enum Input {
    /// Hashed as it arrives.
    Streaming(Box<dyn DynDigest>),
    /// Kept whole, for the algorithms that are only computed in one go.
    Buffered(Vec<u8>),
}

impl State {
    pub fn new(algorithm: Algorithm, options: Options, hmac: Option<Hmac>) -> Self {
        let input = match algorithm.streaming() {
            Some(hasher) => Input::Streaming(hasher),
            None => Input::Buffered(Vec::new()),
        };
        let mut state = State {
            algorithm,
            options,
            input,
            hmac: None,
        };
        if let Some(hmac) = hmac {
            state.update(&hmac.inner_pad);
            state.hmac = Some(hmac);
        }
        state
    }

    pub fn update(&mut self, data: &[u8]) {
        match &mut self.input {
            Input::Streaming(hasher) => hasher.update(data),
            Input::Buffered(input) => input.extend_from_slice(data),
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let digest = match self.input {
            Input::Streaming(hasher) => hasher.finalize().into_vec(),
            Input::Buffered(input) => self.algorithm.digest_with(&self.options, &input),
        };
        match self.hmac {
            Some(hmac) => hmac.finish(&digest),
            None => digest,
        }
    }
}

/// The `$context` argument, which must be a `HashContext`. It is declared
/// `mixed` because a parameter type cannot name a class; this check gives
/// PHP's message.
pub fn hash_context<'a>(arguments: &'a [Value], function: &str) -> Result<&'a Object, NativeError> {
    match args::object(arguments, 0) {
        Some(object) if object.class.name() == "HashContext" => Ok(object),
        _ => Err(NativeError::type_error(format!(
            "{function}(): Argument #1 ($context) must be of type HashContext, {} given",
            given(args::value(arguments, 0))
        ))),
    }
}

/// The state in a `HashContext`'s native slot; `hash_final()` empties the
/// slot, after which the context cannot be used.
fn state_mut<'a>(
    native: &'a mut Option<Box<dyn Any>>,
    function: &str,
) -> Result<&'a mut State, NativeError> {
    native
        .as_mut()
        .and_then(|state| state.downcast_mut::<State>())
        .ok_or_else(|| finalized(function))
}

/// Takes the state out of a `HashContext`, which leaves it finalized until
/// [`put_state`] puts it back.
pub fn take_state(object: &Object, function: &str) -> Result<State, NativeError> {
    let state = object.native.borrow_mut().take();
    match state.map(|state| state.downcast::<State>()) {
        Some(Ok(state)) => Ok(*state),
        _ => Err(finalized(function)),
    }
}

pub fn put_state(object: &Object, state: State) {
    *object.native.borrow_mut() = Some(Box::new(state));
}

fn finalized(function: &str) -> NativeError {
    NativeError::type_error(format!(
        "{function}(): Argument #1 ($context) must be a valid, non-finalized HashContext"
    ))
}

/// A new `HashContext` holding `state`.
fn new_context(context: &mut dyn Context, state: State) -> NativeResult {
    let object = context.create_object("HashContext", PhpArray::new())?;
    let Value::Object(hash_context) = &object else {
        unreachable!("create_object makes an object or fails");
    };
    put_state(hash_context, state);
    Ok(object)
}

/// `hash_init(string $algo, int $flags = 0, string $key = "", array $options = []): HashContext`
const HASH_INIT: FunctionEntry = FunctionEntry::sync(
    "hash_init",
    &[
        Param::new("algo", ParamType::String),
        Param::new("flags", ParamType::Int).optional(),
        Param::new("key", ParamType::String).optional(),
        Param::new("options", ParamType::Array).optional(),
    ],
    hash_init,
);

fn hash_init(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::algorithm(args::bytes(arguments, 0), "hash_init")?;
    let flags = args::int(arguments, 1).unwrap_or(0);
    let hmac = if flags & HASH_HMAC != 0 {
        if algorithm.block_size().is_none() {
            return Err(NativeError::value_error(
                "hash_init(): Argument #1 ($algo) must be a cryptographic hashing algorithm \
                 if HMAC is requested",
            ));
        }
        let key = args::bytes(arguments, 2);
        if key.is_empty() {
            return Err(NativeError::value_error(
                "hash_init(): Argument #3 ($key) must not be empty when HMAC is requested",
            ));
        }
        Some(Hmac::new(algorithm, key))
    } else {
        None
    };
    let options = Options::read(context, algorithm, arguments.get(3), "hash_init")?;
    new_context(context, State::new(algorithm, options, hmac))
}

/// `hash_update(HashContext $context, string $data): true`
const HASH_UPDATE: FunctionEntry = FunctionEntry::sync(
    "hash_update",
    &[
        Param::new("context", ParamType::Mixed),
        Param::new("data", ParamType::String),
    ],
    hash_update,
);

fn hash_update(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let object = hash_context(arguments, "hash_update")?;
    let mut native = object.native.borrow_mut();
    state_mut(&mut native, "hash_update")?.update(args::bytes(arguments, 1));
    Ok(Value::Bool(true))
}

/// `hash_final(HashContext $context, bool $binary = false): string`, which
/// finalizes the context.
const HASH_FINAL: FunctionEntry = FunctionEntry::sync(
    "hash_final",
    &[
        Param::new("context", ParamType::Mixed),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    hash_final,
);

fn hash_final(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let object = hash_context(arguments, "hash_final")?;
    let state = take_state(object, "hash_final")?;
    Ok(output(
        state.finish(),
        args::bool(arguments, 1).unwrap_or(false),
    ))
}

/// `hash_copy(HashContext $context): HashContext`
const HASH_COPY: FunctionEntry = FunctionEntry::sync(
    "hash_copy",
    &[Param::new("context", ParamType::Mixed)],
    hash_copy,
);

fn hash_copy(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let object = hash_context(arguments, "hash_copy")?;
    let copy = state_mut(&mut object.native.borrow_mut(), "hash_copy")?.clone();
    new_context(context, copy)
}

/// `hash_update_stream(HashContext $context, resource $stream, int $length = -1): int`
///
/// Phera has no stream resources yet, so every call fails the way PHP
/// fails for a `$stream` that is not one.
const HASH_UPDATE_STREAM: FunctionEntry = FunctionEntry::sync(
    "hash_update_stream",
    &[
        Param::new("context", ParamType::Mixed),
        Param::new("stream", ParamType::Mixed),
        Param::new("length", ParamType::Int).optional(),
    ],
    hash_update_stream,
);

fn hash_update_stream(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    hash_context(arguments, "hash_update_stream")?;
    Err(NativeError::type_error(format!(
        "hash_update_stream(): Argument #2 ($stream) must be of type resource, {} given",
        given(args::value(arguments, 1))
    )))
}
