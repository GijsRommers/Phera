//! `$h=hash_init('sha256'); hash_update($h,'a'); hash_final(hash_copy($h));`.
use crate::{
    algorithms::Algorithm,
    functions,
    hmac::Hmac,
    options::{self, Options},
};
use digest::DynDigest;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value, args};

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

pub fn create(context: &mut dyn Context, state: State) -> NativeResult {
    let value = context.create_object("HashContext", PhpArray::new())?;
    if let Value::Object(object) = &value {
        *object.native.borrow_mut() = Some(Box::new(state));
    }
    Ok(value)
}
pub fn init(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::algorithm(args::bytes(arguments, 0), "hash_init")?;
    let hmac = if args::int(arguments, 1).unwrap_or(0) & 1 != 0 {
        if algorithm.block_size().is_none() {
            return Err(NativeError::value_error(
                "hash_init(): Argument #1 ($algo) must be a cryptographic hashing algorithm \
                if HMAC is requested",
            ));
        }
        if args::bytes(arguments, 2).is_empty() {
            return Err(NativeError::value_error(
                "hash_init(): Argument #3 ($key) must not be empty when HMAC is requested",
            ));
        }
        Some(Hmac::new(algorithm, args::bytes(arguments, 2)))
    } else {
        None
    };
    let options = Options::read(context, algorithm, arguments.get(3), "hash_init")?;
    create(context, State::new(algorithm, options, hmac))
}
pub fn object<'a>(arguments: &'a [Value], function: &str) -> Result<&'a Object, NativeError> {
    if let Some(object) = args::object(arguments, 0)
        && object.class.name() == "HashContext"
    {
        return Ok(object);
    }
    let given = arguments.first().map_or("null".into(), Value::type_name);
    Err(NativeError::type_error(format!(
        "{function}(): Argument #1 ($context) must be of type HashContext, {given} given"
    )))
}
pub fn invalid(function: &str) -> NativeError {
    NativeError::type_error(format!(
        "{function}(): Argument #1 ($context) must be a valid, non-finalized HashContext"
    ))
}
pub fn update(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let object = object(arguments, "hash_update")?;
    let mut native = object.native.borrow_mut();
    let state = native
        .as_mut()
        .and_then(|s| s.downcast_mut::<State>())
        .ok_or_else(|| invalid("hash_update"))?;
    state.update(args::bytes(arguments, 1));
    Ok(Value::Bool(true))
}
pub fn finish(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let object = object(arguments, "hash_final")?;
    let state = object
        .native
        .borrow_mut()
        .take()
        .and_then(|s| s.downcast::<State>().ok())
        .ok_or_else(|| invalid("hash_final"))?;
    Ok(functions::output(
        state.finish(),
        args::bool(arguments, 1).unwrap_or(false),
    ))
}
pub fn copy(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let object = object(arguments, "hash_copy")?;
    let native = object.native.borrow();
    let state = native
        .as_ref()
        .and_then(|s| s.downcast_ref::<State>())
        .ok_or_else(|| invalid("hash_copy"))?;
    create(context, state.clone())
}
pub fn stream(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    object(arguments, "hash_update_stream")?;
    Err(NativeError::type_error(format!(
        "hash_update_stream(): Argument #2 ($stream) must be of type resource, {} given",
        arguments.get(1).map_or("null".into(), Value::type_name)
    )))
}
pub fn construct(
    _: &mut dyn Context,
    _: Option<&std::rc::Rc<Object>>,
    _: &[Value],
) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct HashContext, use hash_init() instead",
    ))
}
