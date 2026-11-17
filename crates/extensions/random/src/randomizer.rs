//! `$r = new Random\Randomizer($engine)` shares the supplied engine object.
use crate::{collections, engine_methods, engines::Engine, floats, procedural};
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use std::rc::Rc;
#[derive(Clone)]
struct Randomizer {
    engine: Rc<Object>,
}
pub fn construct(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let engine = match args::object(arguments, 0) {
        Some(engine) => engine.clone(),
        None => {
            let Value::Object(engine) =
                context.create_object("Random\\Engine\\Secure", PhpArray::new())?
            else {
                return Err(NativeError::error("Unable to create random engine"));
            };
            *engine.native.borrow_mut() = Some(Box::new(Engine::Secure));
            engine
        }
    };
    if !engine.class.name().starts_with("Random\\Engine\\") {
        return Err(NativeError::type_error(format!(
            "Random\\Randomizer::__construct(): Argument #1 ($engine) must be of type \
            ?Random\\Engine, {} given",
            engine.class.name()
        )));
    }
    let object = this.ok_or_else(|| NativeError::error("Missing Randomizer object"))?;
    object.set(ArrayKey::str("engine"), Value::Object(engine.clone()));
    *object.native.borrow_mut() = Some(Box::new(Randomizer { engine }));
    Ok(Value::Null)
}
fn with_engine(
    this: Option<&Rc<Object>>,
    run: impl FnOnce(&mut Engine) -> NativeResult,
) -> NativeResult {
    let object = this.ok_or_else(|| NativeError::error("Missing Randomizer object"))?;
    let native = object.native.borrow();
    let state = native
        .as_ref()
        .and_then(|s| s.downcast_ref::<Randomizer>())
        .ok_or_else(|| NativeError::error("Randomizer is not initialized"))?;
    engine_methods::with_engine(Some(&state.engine), run)
}
pub fn next_int(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        Ok(Value::Int((engine.generate()?.0 >> 1) as i64))
    })
}
pub fn next_float(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        Ok(Value::Float(
            (engine.word64()? >> 11) as f64 / 9007199254740992.0,
        ))
    })
}
pub fn get_int(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let min = args::int(arguments, 0).unwrap_or_default();
    let max = args::int(arguments, 1).unwrap_or_default();
    if min > max {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getInt(): Argument #2 ($max) must be greater than or equal to \
            argument #1 ($min)",
        ));
    }
    with_engine(this, |engine| Ok(Value::Int(engine.integer(min, max)?)))
}
pub fn get_float(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let min = args::float(arguments, 0).unwrap_or_default();
    let max = args::float(arguments, 1).unwrap_or_default();
    for (number, name, value) in [(1, "min", min), (2, "max", max)] {
        if !value.is_finite() {
            return Err(NativeError::value_error(format!(
                "Random\\Randomizer::getFloat(): Argument #{number} (${name}) must be finite"
            )));
        }
    }
    if max <= min {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than \
                argument #1 ($min)",
        ));
    }
    if arguments.len() > 2 {
        return Err(NativeError::type_error(format!(
            "Random\\Randomizer::getFloat(): Argument #3 ($boundary) must be of type \
            Random\\IntervalBoundary, {} given",
            arguments[2].type_name()
        )));
    }
    with_engine(this, |engine| {
        Ok(Value::Float(floats::closed_open(engine, min, max)?))
    })
}
pub fn bytes(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 0).unwrap_or_default();
    if length < 1 {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getBytes(): Argument #1 ($length) must be greater than 0",
        ));
    }
    with_engine(this, |engine| {
        let mut result = procedural::buffer(length as usize)?;
        let mut position = 0;
        while position < result.len() {
            let (word, size) = engine.generate()?;
            let count = size.min(result.len() - position);
            result[position..position + count].copy_from_slice(&word.to_le_bytes()[..count]);
            position += count;
        }
        Ok(Value::from(result))
    })
}
pub fn from_string(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let source = args::bytes(arguments, 0);
    let length = args::int(arguments, 1).unwrap_or_default();
    if source.is_empty() {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getBytesFromString(): Argument #1 ($string) must not be empty",
        ));
    }
    if length < 1 {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getBytesFromString(): Argument #2 ($length) must be \
                greater than 0",
        ));
    }
    with_engine(this, |engine| {
        let mut result = procedural::buffer(length as usize)?;
        let max = source.len() - 1;
        if max > 255 {
            for byte in &mut result {
                *byte = source[engine.range(0, max as i64)? as usize];
            }
        } else {
            let mut mask = max;
            mask |= mask >> 1;
            mask |= mask >> 2;
            mask |= mask >> 4;
            let mut position = 0;
            let mut failures = 0;
            while position < result.len() {
                let (word, size) = engine.generate()?;
                for byte in &word.to_le_bytes()[..size] {
                    let offset = usize::from(*byte) & mask;
                    if offset > max {
                        failures += 1;
                        if failures > 50 {
                            return Err(NativeError::throw(
                                "Random\\BrokenRandomEngineError",
                                "Failed to generate an acceptable random number in 50 attempts",
                            ));
                        }
                        continue;
                    }
                    failures = 0;
                    result[position] = source[offset];
                    position += 1;
                    if position == result.len() {
                        break;
                    }
                }
            }
        }
        Ok(Value::from(result))
    })
}
pub fn shuffle_array(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    with_engine(this, |engine| {
        collections::array(engine, args::array(arguments, 0).expect("prepared array"))
    })
}
pub fn shuffle_bytes(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    with_engine(this, |engine| {
        let mut bytes = args::bytes(arguments, 0).to_vec();
        collections::shuffle(engine, &mut bytes)?;
        Ok(Value::from(bytes))
    })
}
pub fn pick_keys(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    with_engine(this, |engine| {
        collections::keys(
            engine,
            args::array(arguments, 0).expect("prepared array"),
            args::int(arguments, 1).unwrap_or_default(),
            "Random\\Randomizer::pickArrayKeys",
            true,
        )
    })
}
