//! Array key helpers for the interpreter loop's fast element reads and writes.

use phera_bytecode::Reg;
use phera_value::{ArrayKey, InternedStr, PhpArray, PhpString, Value};

use crate::calls::execution_loop::{Machine, Position};

/// A borrowed integer or string array key from a VM register, avoiding any
/// `PhpString` clone or refcount traffic on array lookups.
#[derive(Clone, Copy)]
pub(super) enum FastKey<'a> {
    Int(i64),
    Interned(&'static InternedStr),
    Str(&'a PhpString),
}

impl FastKey<'_> {
    /// Looks up this key in `array` using the fastest available lookup path
    /// (direct integer hash, precomputed interned hash, or borrowed bytes).
    #[inline]
    pub(super) fn get<'v>(self, array: &'v PhpArray) -> Option<&'v Value> {
        match self {
            Self::Int(integer) => array.get_int(integer),
            Self::Interned(string) => array.get_interned(string),
            Self::Str(string) => array.get_str(string),
        }
    }
}

impl Machine<'_> {
    /// An integer or string key in a register, borrowed without cloning [`PhpString`].
    #[inline]
    pub(super) fn array_key_ref(&self, position: Position, register: Reg) -> Option<FastKey<'_>> {
        match self
            .registers
            .window(position.frame.base() + register.index(), 1)
        {
            [Some(Value::Int(integer))] => Some(FastKey::Int(*integer)),
            [Some(Value::InternedStr(string))] => Some(FastKey::Interned(string)),
            [Some(Value::String(string))] => Some(FastKey::Str(string)),
            _ => None,
        }
    }

    /// An integer or string key in a register, as PHP converts it.
    pub(super) fn array_key(&self, position: Position, register: Reg) -> Option<ArrayKey> {
        match self
            .registers
            .window(position.frame.base() + register.index(), 1)
        {
            [Some(Value::Int(integer))] => Some(ArrayKey::Int(*integer)),
            [Some(Value::InternedStr(string))] => Some(string.to_array_key()),
            [Some(Value::String(string))] => Some(ArrayKey::from_string(string.clone())),
            _ => None,
        }
    }
}
