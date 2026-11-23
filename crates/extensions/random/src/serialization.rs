//! `unserialize(serialize($engine))` resumes a seeded engine at the same draw.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, MethodEntry, NativeError, NativeResult, Object, Param, ParamType, PhpArray,
    Value, args,
};

use crate::engine_classes::with_engine;
use crate::engines::Engine;
use crate::mt19937::{Mt19937, StateMode};

pub(crate) const SERIALIZE: MethodEntry =
    MethodEntry::public("__serialize", &[], serialize).returning("array");
pub(crate) const UNSERIALIZE: MethodEntry = MethodEntry::public(
    "__unserialize",
    &[Param::new("data", ParamType::Array)],
    unserialize,
)
.returning("void");

fn serialize(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let object = this.ok_or_else(|| NativeError::error("Random engine method needs a receiver"))?;
    let state = with_engine(this, |engine| {
        let mut data = PhpArray::new();
        match engine {
            Engine::Mt(engine) => {
                let (words, index, mode) = engine.snapshot();
                for (index, word) in words.iter().enumerate() {
                    data.insert(ArrayKey::Int(index as i64), hex(&word.to_le_bytes()));
                }
                data.insert(ArrayKey::Int(words.len() as i64), Value::Int(index as i64));
                data.insert(
                    ArrayKey::Int(words.len() as i64 + 1),
                    Value::Int(mode.php_id()),
                );
            }
            Engine::Pcg(state) => {
                data.insert(
                    ArrayKey::Int(0),
                    hex(&((*state >> 64) as u64).to_le_bytes()),
                );
                data.insert(ArrayKey::Int(1), hex(&(*state as u64).to_le_bytes()));
            }
            Engine::Xoshiro(state) => {
                for (index, word) in state.iter().enumerate() {
                    data.insert(ArrayKey::Int(index as i64), hex(&word.to_le_bytes()));
                }
            }
            Engine::Secure => {
                return Err(NativeError::throw("Exception", "Engine serialize failed"));
            }
        }
        Ok(Value::Array(Rc::new(data)))
    })?;
    Ok(Value::Array(Rc::new(
        [
            (ArrayKey::Int(0), Value::Array(Rc::new(object.to_array()))),
            (ArrayKey::Int(1), state),
        ]
        .into_iter()
        .collect(),
    )))
}

fn unserialize(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let object = this.ok_or_else(|| NativeError::error("Random engine method needs a receiver"))?;
    let invalid = || {
        NativeError::throw(
            "Exception",
            format!(
                "Invalid serialization data for {} object",
                object.class.name()
            ),
        )
    };
    let data = args::array(arguments, 0)
        .filter(|data| data.len() == 2)
        .ok_or_else(invalid)?;
    let members = array_at(data, 0).ok_or_else(invalid)?;
    let state = array_at(data, 1).ok_or_else(invalid)?;
    let engine = parse_state(object.class.name(), state).ok_or_else(invalid)?;
    let mut native = object.native.try_borrow_mut().map_err(|_| invalid())?;
    object.update(|properties| {
        for (key, value) in members.iter() {
            properties.insert(key, value.clone());
        }
    });
    *native = Some(Box::new(engine));
    Ok(Value::Null)
}

fn array_at(values: &PhpArray, index: i64) -> Option<&PhpArray> {
    match values.get(&ArrayKey::Int(index))? {
        Value::Array(array) => Some(array),
        _ => None,
    }
}

fn parse_state(class: &str, values: &PhpArray) -> Option<Engine> {
    match class {
        "Random\\Engine\\Mt19937" if values.len() == 626 => {
            let mut words = [0; 624];
            for (index, word) in words.iter_mut().enumerate() {
                *word = u32::from_le_bytes(hex_at(values, index as i64)?);
            }
            let (Value::Int(index), Value::Int(mode)) = (
                values.get(&ArrayKey::Int(624))?,
                values.get(&ArrayKey::Int(625))?,
            ) else {
                return None;
            };
            let mode = StateMode::from_php_id(*mode as u32)?;
            Some(Engine::Mt(Box::new(Mt19937::from_snapshot(
                words,
                *index as u32,
                mode,
            )?)))
        }
        "Random\\Engine\\PcgOneseq128XslRr64" if values.len() == 2 => {
            let high = u128::from(u64::from_le_bytes(hex_at(values, 0)?));
            let low = u128::from(u64::from_le_bytes(hex_at(values, 1)?));
            Some(Engine::Pcg((high << 64) | low))
        }
        "Random\\Engine\\Xoshiro256StarStar" if values.len() == 4 => {
            let mut words = [0; 4];
            for (index, word) in words.iter_mut().enumerate() {
                *word = u64::from_le_bytes(hex_at(values, index as i64)?);
            }
            (words != [0; 4]).then_some(Engine::Xoshiro(words))
        }
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> Value {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize]);
        output.push(DIGITS[(byte & 15) as usize]);
    }
    Value::from(output)
}

fn hex_at<const N: usize>(values: &PhpArray, index: i64) -> Option<[u8; N]> {
    let hex = values.get(&ArrayKey::Int(index))?.as_str_bytes()?;
    if hex.len() != N * 2 {
        return None;
    }
    let mut decoded = [0; N];
    for (pair, byte) in hex.as_chunks::<2>().0.iter().zip(decoded.iter_mut()) {
        let high = (pair[0] as char).to_digit(16)? as u8;
        let low = (pair[1] as char).to_digit(16)? as u8;
        *byte = high << 4 | low;
    }
    Some(decoded)
}
