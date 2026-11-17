//! `shuffle`, `str_shuffle` and `array_rand` use the request's MT state.
//! Selection order follows php-src/ext/standard/array.c and string.c (LICENSE-PHP).
use crate::{engines::Engine, procedural};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};
pub fn shuffle<T>(engine: &mut Engine, values: &mut [T]) -> Result<(), NativeError> {
    for i in (1..values.len()).rev() {
        let other = engine.range(0, i as i64)? as usize;
        values.swap(i, other);
    }
    Ok(())
}
pub fn array(engine: &mut Engine, array: &PhpArray) -> NativeResult {
    let mut values: Vec<Value> = array.iter().map(|(_, v)| v.clone()).collect();
    shuffle(engine, &mut values)?;
    list(values)
}
pub fn list(values: impl IntoIterator<Item = Value>) -> NativeResult {
    let mut result = PhpArray::new();
    for value in values {
        result.push(value)?;
    }
    Ok(Value::from(result))
}
pub fn keys(
    engine: &mut Engine,
    array: &PhpArray,
    count: i64,
    name: &str,
    always_array: bool,
) -> NativeResult {
    if array.is_empty() {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #1 ($array) must not be empty"
        )));
    }
    if count < 1 || count as usize > array.len() {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)"
        )));
    }
    let keys: Vec<Value> = array
        .iter()
        .map(|(k, _)| match k {
            ArrayKey::Int(i) => Value::Int(*i),
            ArrayKey::String(s) => Value::String(s.clone()),
        })
        .collect();
    if count == 1 {
        // PHP samples live bucket positions when at least half its buckets are occupied.
        // Positional iteration preserves holes left by unset(), unlike a dense values vector.
        let mut used = 0;
        let mut position = array.first_position();
        while let Some(index) = position {
            used = index + 1;
            position = array.next_position(index);
        }
        let key = if array.len() < used - (used >> 1) {
            keys[engine.range(0, keys.len() as i64 - 1)? as usize].clone()
        } else {
            loop {
                let index = engine.range(0, used as i64 - 1)? as usize;
                if let Some((key, _)) = array.entry_at(index) {
                    break match key {
                        ArrayKey::Int(key) => Value::Int(*key),
                        ArrayKey::String(key) => Value::String(key.clone()),
                    };
                }
            }
        };
        return if always_array { list([key]) } else { Ok(key) };
    }
    let invert = count as usize > keys.len() / 2;
    let mut remaining = if invert {
        keys.len() - count as usize
    } else {
        count as usize
    };
    let mut selected = vec![false; keys.len()];
    let mut failures = 0;
    while remaining > 0 {
        let index = engine.range(0, keys.len() as i64 - 1)? as usize;
        if selected[index] {
            failures += 1;
            if failures > 50 {
                return Err(NativeError::throw(
                    "Random\\BrokenRandomEngineError",
                    "Failed to generate an acceptable random number in 50 attempts",
                ));
            }
        } else {
            selected[index] = true;
            remaining -= 1;
            failures = 0;
        }
    }
    list(
        keys.into_iter()
            .enumerate()
            .filter(|(i, _)| selected[*i] ^ invert)
            .map(|(_, v)| v),
    )
}
pub fn shuffle_array(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(Value::Reference(reference)) = arguments.first() else {
        return Err(NativeError::error("shuffle requires a reference"));
    };
    let value = reference.borrow().clone();
    let Value::Array(array) = value else {
        return Err(NativeError::type_error(
            "shuffle(): Argument #1 ($array) must be of type array",
        ));
    };
    let result = self::array(procedural::generator(context)?, &array)?;
    *reference.borrow_mut() = result;
    Ok(Value::Bool(true))
}
pub fn shuffle_string(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut bytes = args::bytes(arguments, 0).to_vec();
    shuffle(procedural::generator(context)?, &mut bytes)?;
    Ok(Value::from(bytes))
}
pub fn array_rand(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = args::array(arguments, 0).expect("prepared array");
    keys(
        procedural::generator(context)?,
        array,
        args::int(arguments, 1).unwrap_or(1),
        "array_rand",
        false,
    )
}
