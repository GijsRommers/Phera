//! Secretstream state is an opaque PHP string passed by reference.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};
use zeroize::Zeroizing;

use crate::{
    argument, exception, fixed, keygen, list,
    opaque_state::{replace_state, with_state},
    random,
    secretstream_state::{MAX_MESSAGE, OVERHEAD, STATE_LEN, Secretstream},
    zeroed,
};

const INIT_PUSH: &str = "sodium_crypto_secretstream_xchacha20poly1305_init_push";
const INIT_PULL: &str = "sodium_crypto_secretstream_xchacha20poly1305_init_pull";
const PUSH: &str = "sodium_crypto_secretstream_xchacha20poly1305_push";
const PULL: &str = "sodium_crypto_secretstream_xchacha20poly1305_pull";
const REKEY: &str = "sodium_crypto_secretstream_xchacha20poly1305_rekey";
const KEY_BYTES: &str = "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES";

const KEY: Param = Param::new("key", ParamType::String).sensitive();
const STATE: Param = Param::new("state", ParamType::String).raw_by_ref();
const AAD: Param =
    Param::new("additional_data", ParamType::String).with_default(ConstantValue::String(""));

pub(crate) const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_secretstream_xchacha20poly1305_keygen",
            &[],
            keygen::<32>,
        ),
        "string",
    ),
    (FunctionEntry::sync(INIT_PUSH, &[KEY], init_push), "array"),
    (
        FunctionEntry::sync(
            INIT_PULL,
            &[Param::new("header", ParamType::String), KEY],
            init_pull,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            PUSH,
            &[
                STATE,
                Param::new("message", ParamType::String).sensitive(),
                AAD,
                Param::new("tag", ParamType::Int).with_constant_default(
                    "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE",
                    ConstantValue::Int(0),
                ),
            ],
            push,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            PULL,
            &[STATE, Param::new("ciphertext", ParamType::String), AAD],
            pull,
        ),
        "array|false",
    ),
    (FunctionEntry::sync(REKEY, &[STATE], rekey), "void"),
];

fn init_push(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let key = fixed(args::bytes(args, 0), INIT_PUSH, 1, "key", KEY_BYTES)?;
    let header = random::<24>()?;
    let state = Zeroizing::new(Secretstream::new(key, &header).encode());
    list([
        Value::from(state.as_slice()),
        Value::from(header.as_slice()),
    ])
}

fn init_pull(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let header = fixed(
        args::bytes(args, 0),
        INIT_PULL,
        1,
        "header",
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES",
    )?;
    let key = fixed(args::bytes(args, 1), INIT_PULL, 2, "key", KEY_BYTES)?;
    let state = Zeroizing::new(Secretstream::new(key, header).encode());
    Ok(Value::from(state.as_slice()))
}

fn push(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    use_state(args, PUSH, |state, variable| {
        let message = args::bytes(args, 1);
        let frame_len = message.len().checked_add(OVERHEAD)
            .filter(|_| message.len() as u64 <= MAX_MESSAGE)
            .ok_or_else(|| argument(
                PUSH,
                2,
                "message",
                "must be at most SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_MESSAGEBYTES_MAX bytes long",
            ))?;
        let tag = u8::try_from(args::int(args, 3).unwrap_or_default())
            .map_err(|_| argument(PUSH, 4, "tag", "must be in the range of 0-255"))?;
        let mut frame = zeroed(frame_len)?;
        frame[1..1 + message.len()].copy_from_slice(message);
        state.push(&mut frame, args::bytes(args, 2), tag);
        replace_state(variable, state.encode());
        Ok(Value::from(frame))
    })
}

fn pull(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    use_state(args, PULL, |state, variable| {
        let Some((message, tag)) = state.pull(args::bytes(args, 1), args::bytes(args, 2)) else {
            return Ok(Value::Bool(false));
        };
        let result = list([Value::from(message), Value::Int(i64::from(tag))])?;
        replace_state(variable, state.encode());
        Ok(result)
    })
}

fn rekey(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    use_state(args, REKEY, |state, variable| {
        state.rekey();
        replace_state(variable, state.encode());
        Ok(Value::Null)
    })
}

fn use_state(
    args: &[Value],
    function: &str,
    operation: impl FnOnce(&mut Secretstream, &mut Value) -> NativeResult,
) -> NativeResult {
    with_state::<STATE_LEN>(
        args,
        function,
        || invalid_state(function),
        |variable, bytes| {
            let mut state = Secretstream::decode(bytes).ok_or_else(|| invalid_state(function))?;
            operation(&mut state, variable)
        },
    )
}

fn invalid_state(function: &str) -> NativeError {
    if function == PUSH {
        argument(PUSH, 1, "state", "must have a correct length")
    } else {
        exception("incorrect state length")
    }
}
