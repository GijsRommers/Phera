//! Native engine instances own their state: `$engine->generate()` returns little-endian bytes.
use crate::{
    engines::{Engine, fill, secure_u64},
    mt19937::{Mode, Mt19937},
};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

pub fn with_engine(
    this: Option<&Rc<Object>>,
    run: impl FnOnce(&mut Engine) -> NativeResult,
) -> NativeResult {
    let object = this.ok_or_else(|| NativeError::error("Missing random engine object"))?;
    let mut native = object.native.borrow_mut();
    if object.class.name() == "Random\\Engine\\Secure" && native.is_none() {
        *native = Some(Box::new(Engine::Secure));
    }
    let engine = native
        .as_mut()
        .and_then(|s| s.downcast_mut::<Engine>())
        .ok_or_else(|| NativeError::error("Random engine is not initialized"))?;
    run(engine)
}
fn store(this: Option<&Rc<Object>>, engine: Engine) -> NativeResult {
    let object = this.ok_or_else(|| NativeError::error("Missing random engine object"))?;
    *object.native.borrow_mut() = Some(Box::new(engine));
    Ok(Value::Null)
}
pub fn mt(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let mode = match args::int(arguments, 1).unwrap_or(0) {
        0 => Mode::Standard,
        1 => {
            context.deprecated("The MT_RAND_PHP variant of Mt19937 is deprecated".into());
            Mode::Legacy
        }
        _ => {
            return Err(NativeError::value_error(
                "Random\\Engine\\Mt19937::__construct(): Argument #2 ($mode) must be either \
            MT_RAND_MT19937 or MT_RAND_PHP",
            ));
        }
    };
    let seed = match args::int(arguments, 0) {
        Some(seed) => seed as u32,
        None => secure_u64()? as u32,
    };
    store(this, Engine::Mt(Box::new(Mt19937::new(seed, mode))))
}
pub fn pcg(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let seed = match arguments.first() {
        Some(Value::Int(seed)) => *seed as u64 as u128,
        Some(Value::String(seed)) => {
            let bytes = seed.as_bytes();
            if bytes.len() != 16 {
                return Err(NativeError::value_error(
                    "Random\\Engine\\PcgOneseq128XslRr64::__construct(): Argument #1 ($seed) must be a \
            16 byte (128 bit) string",
                ));
            }
            ((u64::from_le_bytes(bytes[..8].try_into().expect("eight bytes")) as u128) << 64)
                | u64::from_le_bytes(bytes[8..].try_into().expect("eight bytes")) as u128
        }
        _ => (u128::from(secure_u64()?) << 64) | u128::from(secure_u64()?),
    };
    store(this, Engine::pcg(seed))
}
pub fn xoshiro(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let engine = match arguments.first() {
        Some(Value::Int(seed)) => Engine::xoshiro(*seed as u64),
        value => {
            let mut random = [0u8; 32];
            let bytes = if let Some(Value::String(seed)) = value {
                seed.as_bytes()
            } else {
                fill(&mut random)?;
                &random
            };
            if bytes.len() != 32 {
                return Err(NativeError::value_error(
                    "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must be a 32 \
            byte (256 bit) string",
                ));
            }
            if bytes.iter().all(|&b| b == 0) {
                return Err(NativeError::value_error(
                    "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must not \
            consist entirely of NUL bytes",
                ));
            }
            let mut state = [0; 4];
            for (word, bytes) in state.iter_mut().zip(bytes.as_chunks::<8>().0) {
                *word = u64::from_le_bytes(*bytes);
            }
            Engine::Xoshiro(state)
        }
    };
    store(this, engine)
}
pub fn generate(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        let (value, size) = engine.generate()?;
        Ok(Value::from(value.to_le_bytes()[..size].to_vec()))
    })
}
pub fn jump(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        if matches!(engine, Engine::Pcg(_)) {
            let advance = args::int(arguments, 0).unwrap_or_default();
            if advance < 0 {
                return Err(NativeError::value_error(
                    "Random\\Engine\\PcgOneseq128XslRr64::jump(): Argument #1 ($advance) must be greater \
            than or equal to 0",
                ));
            }
            engine.jump(advance as u64);
        } else {
            engine.jump_xoshiro(false)?;
        }
        Ok(Value::Null)
    })
}
pub fn jump_long(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        engine.jump_xoshiro(true)?;
        Ok(Value::Null)
    })
}
