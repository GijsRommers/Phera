//! Shuffling and picking keys, for the procedural functions, which draw
//! from the request's Mt19937, and for the Randomizer's methods:
//!
//! ```php
//! mt_srand(42);
//! $a = [1, 2, 3]; shuffle($a);   // the same order as in PHP
//! str_shuffle('abc');
//! array_rand(['a' => 1, 'b' => 2]);           // 'a' or 'b'
//! array_rand(['a' => 1, 'b' => 2, 'c' => 3], 2);  // two keys, in array order
//! ```
//!
//! Selection order follows php-src/ext/standard/array.c and string.c (LICENSE-PHP).

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use crate::engines::{ATTEMPTS, Engine, broken_engine};
use crate::procedural::generator;

pub const FUNCTIONS: &[FunctionEntry] = &[SHUFFLE, STR_SHUFFLE, ARRAY_RAND];

/// Fisher–Yates, from the end, as PHP does it.
pub fn shuffle<T>(engine: &mut Engine, values: &mut [T]) -> Result<(), NativeError> {
    for i in (1..values.len()).rev() {
        let other = engine.range(0, i as i64)? as usize;
        values.swap(i, other);
    }
    Ok(())
}

/// The values of `array` in a shuffled list; the keys are dropped.
pub fn shuffled_list(engine: &mut Engine, array: &PhpArray) -> NativeResult {
    let mut values: Vec<Value> = array.values().cloned().collect();
    shuffle(engine, &mut values)?;
    list(values)
}

pub fn list(values: impl IntoIterator<Item = Value>) -> NativeResult {
    let mut list = PhpArray::new();
    for value in values {
        list.push(value)?;
    }
    Ok(Value::from(list))
}

/// `count` distinct keys of `array`, in the array's order. `function` names
/// the PHP function in errors.
pub fn pick_keys(
    engine: &mut Engine,
    array: &PhpArray,
    count: i64,
    function: &str,
) -> Result<Vec<Value>, NativeError> {
    if array.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($array) must not be empty"
        )));
    }
    if count < 1 || count as usize > array.len() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 ($num) must be between 1 and the number of elements in \
             argument #1 ($array)"
        )));
    }
    if count == 1 {
        return Ok(vec![pick_one_key(engine, array)?]);
    }
    let keys: Vec<Value> = array.keys().map(Value::from).collect();
    // Draw whichever is fewer: the keys to keep or the keys to leave out.
    let invert = count as usize > keys.len() / 2;
    let mut remaining = if invert {
        keys.len() - count as usize
    } else {
        count as usize
    };
    let mut drawn = vec![false; keys.len()];
    let mut repeats = 0;
    while remaining > 0 {
        let index = engine.range(0, keys.len() as i64 - 1)? as usize;
        if drawn[index] {
            repeats += 1;
            if repeats > ATTEMPTS {
                return Err(broken_engine());
            }
        } else {
            drawn[index] = true;
            remaining -= 1;
            repeats = 0;
        }
    }
    let picked = keys
        .into_iter()
        .zip(drawn)
        .filter(|&(_, drawn)| drawn != invert);
    Ok(picked.map(|(key, _)| key).collect())
}

/// One key, drawn the way PHP draws it from its hash table: a slot at
/// random, again until the slot is not a hole left by `unset()`. When
/// fewer than half the slots are live it draws among the live keys
/// instead. Either way the draws match PHP's.
fn pick_one_key(engine: &mut Engine, array: &PhpArray) -> Result<Value, NativeError> {
    // The slots in use: up to and including the last live one.
    let mut slots = 0;
    let mut position = array.first_position();
    while let Some(index) = position {
        slots = index + 1;
        position = array.next_position(index);
    }
    if array.len() < slots - (slots >> 1) {
        let index = engine.range(0, array.len() as i64 - 1)? as usize;
        let key = array.keys().nth(index).expect("index below len()");
        return Ok(Value::from(key));
    }
    loop {
        let slot = engine.range(0, slots as i64 - 1)? as usize;
        if let Some((key, _)) = array.entry_at(slot) {
            return Ok(Value::from(key));
        }
    }
}

/// `shuffle(array &$array): true`
const SHUFFLE: FunctionEntry = FunctionEntry::sync(
    "shuffle",
    &[Param::new("array", ParamType::Array).by_ref()],
    shuffle_function,
);

fn shuffle_function(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let variable = args::reference(arguments, 0).expect("shuffle() takes its array by reference");
    let shuffled = match &*variable.borrow() {
        Value::Array(array) => shuffled_list(generator(context)?, array)?,
        _ => unreachable!("argument preparation checked for an array inside the reference"),
    };
    *variable.borrow_mut() = shuffled;
    Ok(Value::Bool(true))
}

/// `str_shuffle(string $string): string`
const STR_SHUFFLE: FunctionEntry = FunctionEntry::sync(
    "str_shuffle",
    &[Param::new("string", ParamType::String)],
    str_shuffle,
);

fn str_shuffle(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut bytes = args::bytes(arguments, 0).to_vec();
    shuffle(generator(context)?, &mut bytes)?;
    Ok(Value::from(bytes))
}

/// `array_rand(array $array, int $num = 1): int|string|array`: one key
/// itself, more in an array.
const ARRAY_RAND: FunctionEntry = FunctionEntry::sync(
    "array_rand",
    &[
        Param::new("array", ParamType::Array),
        Param::new("num", ParamType::Int).optional(),
    ],
    array_rand,
);

fn array_rand(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = args::array(arguments, 0).expect("argument preparation checked for an array");
    let count = args::int(arguments, 1).unwrap_or(1);
    let mut keys = pick_keys(generator(context)?, array, count, "array_rand")?;
    if count == 1 {
        return Ok(keys.remove(0));
    }
    list(keys)
}
