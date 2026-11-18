//! `Random\Randomizer`, which draws numbers, bytes and selections from the
//! engine it is given, or from a `Secure` one:
//!
//! ```php
//! $randomizer = new Random\Randomizer(new Random\Engine\Mt19937(42));
//! $randomizer->getInt(1, 6);
//! $randomizer->getBytesFromString('0123456789abcdef', 8);
//! $randomizer->pickArrayKeys(['a' => 1, 'b' => 2, 'c' => 3], 2);
//! ```
//!
//! The engine is shared, not copied: drawing from the Randomizer moves the
//! engine object on too.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, PropertyEntry, Value, Visibility, args,
};

use crate::collections::{list, pick_keys, shuffle, shuffled_list};
use crate::engine_classes;
use crate::engines::{ATTEMPTS, Engine, broken_engine};
use crate::procedural::zeroed;
use crate::{floats, given};

/// `nextFloat()` keeps 53 bits, a double's precision, and divides by 2^53.
const TWO_TO_53: f64 = 9007199254740992.0;

/// `final class Random\Randomizer`
pub const RANDOMIZER: ClassEntry = ClassEntry {
    hooks: Some(&phera_extension_api::ClassHooks {
        clone: Some(phera_extension_api::NativeClone::Forbidden),
        ..phera_extension_api::ClassHooks::EMPTY
    }),
    kind: ClassKind::FinalClass,
    properties: &[PropertyEntry::new(
        "engine",
        ConstantValue::Null,
        Visibility::Public,
    )],
    methods: &[
        MethodEntry::public(
            "__construct",
            &[Param::new("engine", ParamType::Mixed).optional()],
            construct,
        ),
        MethodEntry::public("nextInt", &[], next_int),
        MethodEntry::public("nextFloat", &[], next_float),
        MethodEntry::public(
            "getInt",
            &[
                Param::new("min", ParamType::Int),
                Param::new("max", ParamType::Int),
            ],
            get_int,
        ),
        MethodEntry::public(
            "getFloat",
            &[
                Param::new("min", ParamType::Float),
                Param::new("max", ParamType::Float),
                Param::new("boundary", ParamType::Mixed).optional(),
            ],
            get_float,
        ),
        MethodEntry::public(
            "getBytes",
            &[Param::new("length", ParamType::Int)],
            get_bytes,
        ),
        MethodEntry::public(
            "getBytesFromString",
            &[
                Param::new("string", ParamType::String),
                Param::new("length", ParamType::Int),
            ],
            get_bytes_from_string,
        ),
        MethodEntry::public(
            "shuffleArray",
            &[Param::new("array", ParamType::Array)],
            shuffle_array,
        ),
        MethodEntry::public(
            "shuffleBytes",
            &[Param::new("bytes", ParamType::String)],
            shuffle_bytes,
        ),
        MethodEntry::public(
            "pickArrayKeys",
            &[
                Param::new("array", ParamType::Array),
                Param::new("num", ParamType::Int),
            ],
            pick_array_keys,
        ),
    ],
    ..ClassEntry::class("Random\\Randomizer")
};

/// What a Randomizer object holds natively: its engine object.
struct Randomizer {
    engine: Rc<Object>,
}

/// Runs `run` on the engine of the Randomizer `this`.
fn with_engine(
    this: Option<&Rc<Object>>,
    run: impl FnOnce(&mut Engine) -> NativeResult,
) -> NativeResult {
    let object = this.expect("Randomizer methods are instance methods");
    let native = object.native.borrow();
    let randomizer = native
        .as_ref()
        .and_then(|state| state.downcast_ref::<Randomizer>())
        .ok_or_else(|| NativeError::error("Randomizer is not initialized"))?;
    engine_classes::with_engine(Some(&randomizer.engine), run)
}

/// `__construct(?Random\Engine $engine = null)`
fn construct(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    // Declared `mixed`, because a parameter type cannot name an interface;
    // this check gives PHP's message.
    let engine = match args::value(arguments, 0) {
        Value::Null => {
            let Value::Object(engine) =
                context.create_object("Random\\Engine\\Secure", PhpArray::new())?
            else {
                unreachable!("create_object makes an object or fails");
            };
            engine
        }
        Value::Object(engine) if engine.class.name().starts_with("Random\\Engine\\") => {
            engine.clone()
        }
        other => {
            return Err(NativeError::type_error(format!(
                "Random\\Randomizer::__construct(): Argument #1 ($engine) must be of type \
                 ?Random\\Engine, {} given",
                given(other)
            )));
        }
    };
    let object = this.expect("constructors are instance methods");
    object.set(ArrayKey::str("engine"), Value::Object(engine.clone()));
    *object.native.borrow_mut() = Some(Box::new(Randomizer { engine }));
    Ok(Value::Null)
}

/// `nextInt(): int`, one draw shifted down to 63 bits.
fn next_int(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        let (draw, _) = engine.generate()?;
        Ok(Value::Int((draw >> 1) as i64))
    })
}

/// `nextFloat(): float`, in `[0, 1)`.
fn next_float(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        let bits = engine.word64()? >> 11;
        Ok(Value::Float(bits as f64 / TWO_TO_53))
    })
}

/// `getInt(int $min, int $max): int`
fn get_int(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let min = args::int(arguments, 0).unwrap_or_default();
    let max = args::int(arguments, 1).unwrap_or_default();
    if min > max {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getInt(): Argument #2 ($max) must be greater than or equal to \
             argument #1 ($min)",
        ));
    }
    with_engine(this, |engine| Ok(Value::Int(engine.range(min, max)?)))
}

/// `getFloat(float $min, float $max, Random\IntervalBoundary $boundary =
/// IntervalBoundary::ClosedOpen): float`. Phera has no enums yet, so only
/// the default boundary works and any `$boundary` is a `TypeError`.
fn get_float(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
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
    if let Some(boundary) = arguments.get(2) {
        return Err(NativeError::type_error(format!(
            "Random\\Randomizer::getFloat(): Argument #3 ($boundary) must be of type \
             Random\\IntervalBoundary, {} given",
            given(boundary)
        )));
    }
    with_engine(this, |engine| {
        Ok(Value::Float(floats::closed_open(engine, min, max)?))
    })
}

/// `getBytes(int $length): string`: whole draws, little-endian, the last
/// one cut to fit.
fn get_bytes(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 0).unwrap_or_default();
    if length < 1 {
        return Err(NativeError::value_error(
            "Random\\Randomizer::getBytes(): Argument #1 ($length) must be greater than 0",
        ));
    }
    with_engine(this, |engine| {
        let mut bytes = zeroed(length as usize)?;
        let mut filled = 0;
        while filled < bytes.len() {
            let (draw, size) = engine.generate()?;
            let count = size.min(bytes.len() - filled);
            bytes[filled..filled + count].copy_from_slice(&draw.to_le_bytes()[..count]);
            filled += count;
        }
        Ok(Value::from(bytes))
    })
}

/// `getBytesFromString(string $string, int $length): string`
fn get_bytes_from_string(
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
            "Random\\Randomizer::getBytesFromString(): Argument #2 ($length) must be greater \
             than 0",
        ));
    }
    with_engine(this, |engine| {
        let mut result = zeroed(length as usize)?;
        let max = source.len() - 1;
        if max > usize::from(u8::MAX) {
            for byte in &mut result {
                *byte = source[engine.range(0, max as i64)? as usize];
            }
        } else {
            pick_bytes(engine, source, &mut result)?;
        }
        Ok(Value::from(result))
    })
}

/// For up to 256 source bytes, PHP spends one random byte per pick: it
/// masks the byte to the bits `source.len() - 1` needs and draws again
/// when the result is too large.
fn pick_bytes(engine: &mut Engine, source: &[u8], result: &mut [u8]) -> Result<(), NativeError> {
    let max = source.len() - 1;
    let mut mask = max;
    mask |= mask >> 1;
    mask |= mask >> 2;
    mask |= mask >> 4;
    let mut picked = 0;
    let mut repeats = 0;
    while picked < result.len() {
        let (draw, size) = engine.generate()?;
        for byte in &draw.to_le_bytes()[..size] {
            let offset = usize::from(*byte) & mask;
            if offset > max {
                repeats += 1;
                if repeats > ATTEMPTS {
                    return Err(broken_engine());
                }
                continue;
            }
            repeats = 0;
            result[picked] = source[offset];
            picked += 1;
            // The rest of the draw is thrown away.
            if picked == result.len() {
                break;
            }
        }
    }
    Ok(())
}

/// `shuffleArray(array $array): array`, a list.
fn shuffle_array(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let array = args::array(arguments, 0).expect("argument preparation checked for an array");
    with_engine(this, |engine| shuffled_list(engine, array))
}

/// `shuffleBytes(string $bytes): string`
fn shuffle_bytes(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let mut bytes = args::bytes(arguments, 0).to_vec();
    with_engine(this, |engine| {
        shuffle(engine, &mut bytes)?;
        Ok(Value::from(bytes))
    })
}

/// `pickArrayKeys(array $array, int $num): array`, even for one key.
fn pick_array_keys(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let array = args::array(arguments, 0).expect("argument preparation checked for an array");
    let count = args::int(arguments, 1).unwrap_or_default();
    with_engine(this, |engine| {
        let function = "Random\\Randomizer::pickArrayKeys";
        list(pick_keys(engine, array, count, function)?)
    })
}
