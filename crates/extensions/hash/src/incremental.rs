//! `$h=hash_init('sha256'); hash_update($h,'a'); hash_final(hash_copy($h));`.
use crate::{algorithms::Algorithm, functions, options};
use digest::DynDigest;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value, args};

#[derive(Clone)]
pub struct State {
    pub algorithm: Algorithm,
    digest: Option<Box<dyn DynDigest>>,
    buffered: Vec<u8>,
    key: Option<Vec<u8>>,
}
impl State {
    pub fn new(algorithm: Algorithm, key: Option<Vec<u8>>) -> Self {
        let mut digest = algorithm.crypto();
        let key = key.map(|key| {
            let size = algorithm.block_size().expect("validated HMAC");
            let key = if key.len() > size {
                algorithm.digest(&key)
            } else {
                key
            };
            let mut inner = vec![0x36; size];
            let mut outer = vec![0x5c; size];
            for (i, byte) in key.iter().enumerate() {
                inner[i] ^= byte;
                outer[i] ^= byte;
            }
            if let Some(digest) = &mut digest {
                digest.update(&inner);
            }
            outer
        });
        Self {
            algorithm,
            digest,
            buffered: Vec::new(),
            key,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        if let Some(digest) = &mut self.digest {
            digest.update(data);
        } else {
            self.buffered.extend_from_slice(data);
        }
    }
    pub fn finish(self) -> Vec<u8> {
        let inner = if let Some(digest) = self.digest {
            digest.finalize().to_vec()
        } else if let Some(outer) = &self.key {
            let mut inner: Vec<u8> = outer.iter().map(|byte| byte ^ 0x6a).collect();
            inner.extend_from_slice(&self.buffered);
            self.algorithm.digest(&inner)
        } else {
            self.algorithm.digest(&self.buffered)
        };
        if let Some(mut outer) = self.key {
            outer.extend_from_slice(&inner);
            self.algorithm.digest(&outer)
        } else {
            inner
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
    let mut algorithm = options::algorithm(arguments, "hash_init", false)?;
    let key = if args::int(arguments, 1).unwrap_or(0) & 1 != 0 {
        if algorithm.block_size().is_none() {
            return Err(NativeError::value_error(
                "hash_init(): Argument #1 ($algo) must be a cryptographic hashing algorithm if HMAC is requested",
            ));
        }
        if args::bytes(arguments, 2).is_empty() {
            return Err(NativeError::value_error(
                "hash_init(): Argument #3 ($key) must not be empty when HMAC is requested",
            ));
        }
        Some(args::bytes(arguments, 2).to_vec())
    } else {
        None
    };
    options::apply(context, &mut algorithm, arguments.get(3), "hash_init")?;
    create(context, State::new(algorithm, key))
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
