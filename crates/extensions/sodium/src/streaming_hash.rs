//! `sodium_crypto_generichash_final($state)` returns the digest and sets the state to null.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, Value, args,
};
use zeroize::Zeroizing;

use crate::{
    blake_state::{BlakeState, STATE_LEN},
    exception,
    hash::{hash_key, hash_length},
    opaque_state::{erase_string, replace_state, with_state},
};

const INIT: &str = "sodium_crypto_generichash_init";
const UPDATE: &str = "sodium_crypto_generichash_update";
const FINAL: &str = "sodium_crypto_generichash_final";

const LENGTH: Param = Param::new("length", ParamType::Int)
    .with_constant_default("SODIUM_CRYPTO_GENERICHASH_BYTES", ConstantValue::Int(32));
const STATE: Param = Param::new("state", ParamType::String).raw_by_ref();

pub(crate) const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            INIT,
            &[
                Param::new("key", ParamType::String)
                    .sensitive()
                    .with_default(ConstantValue::String("")),
                LENGTH,
            ],
            init,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            UPDATE,
            &[STATE, Param::new("message", ParamType::String)],
            update,
        ),
        "true",
    ),
    (
        FunctionEntry::sync(FINAL, &[STATE, LENGTH], finalize),
        "string",
    ),
];

fn init(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let length = hash_length(args, 1)?;
    let key = hash_key(args, 0)?;
    let bytes = Zeroizing::new(BlakeState::new(key, length).encode());
    Ok(Value::from(bytes.as_slice()))
}

fn update(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    with_state::<STATE_LEN>(
        args,
        UPDATE,
        || exception("incorrect state length"),
        |variable, bytes| {
            let mut state =
                BlakeState::decode(bytes).ok_or_else(|| exception("incorrect state length"))?;
            state
                .update(args::bytes(args, 1))
                .ok_or_else(|| exception("internal error"))?;
            replace_state(variable, state.encode());
            Ok(Value::Bool(true))
        },
    )
}

fn finalize(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    with_state::<STATE_LEN>(
        args,
        FINAL,
        || exception("incorrect state length"),
        |variable, bytes| {
            let state =
                BlakeState::decode(bytes).ok_or_else(|| exception("incorrect state length"))?;
            let length = hash_length(args, 1)?;
            let result = Zeroizing::new(state.finish().ok_or_else(|| exception("internal error"))?);
            erase_string(variable);
            Ok(Value::from(&result[..length]))
        },
    )
}
