use std::{cell::RefCell, rc::Rc};

use phera_ext_sodium::Sodium;
use phera_extension_api::{
    ArrayKey, ClassEntry, NativeResult, PhpArray, Registry, Value,
    testing::{TestContext, call, oracle},
};

const HASH_INIT: &str = "sodium_crypto_generichash_init";
const HASH_UPDATE: &str = "sodium_crypto_generichash_update";
const HASH_FINAL: &str = "sodium_crypto_generichash_final";
const STREAM_INIT_PUSH: &str = "sodium_crypto_secretstream_xchacha20poly1305_init_push";
const STREAM_INIT_PULL: &str = "sodium_crypto_secretstream_xchacha20poly1305_init_pull";
const STREAM_PUSH: &str = "sodium_crypto_secretstream_xchacha20poly1305_push";
const STREAM_PULL: &str = "sodium_crypto_secretstream_xchacha20poly1305_pull";
const STREAM_REKEY: &str = "sodium_crypto_secretstream_xchacha20poly1305_rekey";

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.class(ClassEntry::class("Exception"));
    registry.load(&Sodium);
    registry
}

fn invoke(registry: &Registry, name: &str, args: Vec<Value>) -> Value {
    native(registry, name, args).unwrap()
}

fn native(registry: &Registry, name: &str, args: Vec<Value>) -> NativeResult {
    call(registry, &mut TestContext::default(), name, args)
}

fn as_ref(value: Value) -> Value {
    Value::Reference(Rc::new(RefCell::new(value)))
}

fn bytes(value: &Value) -> Vec<u8> {
    let Value::String(bytes) = &*value.deref() else {
        panic!("expected a string, got {value:?}");
    };
    bytes.as_bytes().to_vec()
}

fn list<const N: usize>(items: [Value; N]) -> Value {
    let mut array = PhpArray::new();
    for item in items {
        array.push(item).unwrap();
    }
    Value::from(array)
}

fn index(value: &Value, key: i64) -> Value {
    let Value::Array(array) = value else {
        panic!("expected an array")
    };
    array.get(&ArrayKey::Int(key)).unwrap().clone()
}

#[test]
fn streaming_blake2b_matches_php_for_copied_states_and_chunk_boundaries() {
    if oracle::find_php().is_none() {
        return;
    }
    let registry = registry();
    let script = format!(
        "<?php\n{}\n{}",
        oracle::PHP_PREAMBLE,
        r#"
foreach ([0, 16, 32, 64] as $keyLength) {
    for ($length = 16; $length <= 64; ++$length) {
        foreach ([0, 1, 127, 128, 129, 255, 256, 257] as $split) {
            run('hash', function () use ($keyLength, $length, $split) {
                $state = sodium_crypto_generichash_init(str_repeat("\xa5", $keyLength), $length);
                $size = strlen($state);
                sodium_crypto_generichash_update($state, str_repeat("\x9b", $split));
                $copy = $state;
                for ($i = 0; $i < 60; ++$i) {
                    sodium_crypto_generichash_update($state, str_repeat(chr($i), $i % 5));
                }
                sodium_crypto_generichash_update($copy, "\x00alternate");
                $digest = sodium_crypto_generichash_final($state, $length);
                $other = sodium_crypto_generichash_final($copy, 80 - $length);
                return [$size, $digest, $other, $state, $copy];
            });
        }
    }
}
"#
    );
    let php = oracle::run_php("sodium streaming BLAKE2b", &script).unwrap();
    let mut labels = Vec::new();
    let mut actual = Vec::new();
    for key_length in [0, 16, 32, 64] {
        for length in 16..=64 {
            for split in [0, 1, 127, 128, 129, 255, 256, 257] {
                let state = as_ref(invoke(
                    &registry,
                    HASH_INIT,
                    vec![Value::from(vec![0xa5; key_length]), Value::Int(length)],
                ));
                let size = bytes(&state).len();
                assert_eq!(size, 384);
                let update = |state: &Value, message| {
                    assert!(matches!(
                        invoke(&registry, HASH_UPDATE, vec![state.clone(), message]),
                        Value::Bool(true)
                    ));
                };
                update(&state, Value::from(vec![0x9b; split]));
                let copy = as_ref((*state.deref()).clone());
                for i in 0..60 {
                    update(&state, Value::from(vec![i; usize::from(i % 5)]));
                }
                update(&copy, Value::from("\0alternate"));
                let digest = invoke(
                    &registry,
                    HASH_FINAL,
                    vec![state.clone(), Value::Int(length)],
                );
                let other = invoke(
                    &registry,
                    HASH_FINAL,
                    vec![copy.clone(), Value::Int(80 - length)],
                );
                labels.push(format!(
                    "key length {key_length}, length {length}, split {split}"
                ));
                actual.push(format!(
                    "hash={}",
                    oracle::encode(&list([
                        Value::Int(size as i64),
                        digest,
                        other,
                        (*state.deref()).clone(),
                        (*copy.deref()).clone()
                    ]))
                ));
            }
        }
    }
    oracle::compare("sodium streaming BLAKE2b", &labels, &php, &actual);
}

const STREAM_PHP: &str = r#"
function send_frames(&$s) {
    $frames = [];
    $cases = stream_cases();
    foreach ($cases as $i => [$message, $aad, $tag]) {
        if ($i === 3) sodium_crypto_secretstream_xchacha20poly1305_rekey($s);
        $frames[] = sodium_crypto_secretstream_xchacha20poly1305_push($s, $message, $aad, $tag);
    }
    return $frames;
}
function stream_cases() {
    return [
        ['', '', 0],
        [str_repeat("\xa5", 31), str_repeat("\x1b", 17), 1],
        [str_repeat("\x11", 64), str_repeat("\xfe", 32), 2],
        [str_repeat("\x80", 65), '\0', 255],
        ['last message', '', 3],
    ];
}
"#;

fn stream_frames() -> [(Vec<u8>, Vec<u8>, i64); 5] {
    [
        (vec![], vec![], 0),
        (vec![0xa5; 31], vec![0x1b; 17], 1),
        (vec![0x11; 64], vec![0xfe; 32], 2),
        (vec![0x80; 65], b"\\0".to_vec(), 255),
        (b"last message".to_vec(), vec![], 3),
    ]
}

fn encrypt(registry: &Registry, state: &Value) -> Vec<Value> {
    stream_frames()
        .into_iter()
        .enumerate()
        .map(|(i, (msg, aad, tag))| {
            if i == 3 {
                assert!(invoke(registry, STREAM_REKEY, vec![state.clone()]).is_null());
            }
            invoke(
                registry,
                STREAM_PUSH,
                vec![
                    state.clone(),
                    Value::from(msg),
                    Value::from(aad),
                    Value::Int(tag),
                ],
            )
        })
        .collect()
}

#[test]
fn secretstream_matches_fixed_wire_vector_and_interoperates_through_rekeys() {
    if oracle::find_php().is_none() {
        return;
    }
    let registry = registry();
    let key = Value::from((0_u8..32).collect::<Vec<_>>());
    let header = Value::from((32_u8..56).collect::<Vec<_>>());
    let state = as_ref(invoke(
        &registry,
        STREAM_INIT_PULL,
        vec![header.clone(), key.clone()],
    ));
    let fixed = encrypt(&registry, &state);
    let from_php = oracle::run_php(
        "sodium secretstream encryption",
        &format!(
            "<?php\n{STREAM_PHP}\n$key = {};\n$state = {STREAM_INIT_PULL}({}, $key);\n
             foreach (send_frames($state) as $frame) echo bin2hex($frame), \"\\n\";\n
             [$state, $header] = {STREAM_INIT_PUSH}($key);\n
             echo bin2hex($header), \"\\n\";\n
             foreach (send_frames($state) as $frame) echo bin2hex($frame), \"\\n\";",
            oracle::php_literal(&key),
            oracle::php_literal(&header),
        ),
    )
    .unwrap();
    let (fixed_php, php_stream) = from_php.split_at(fixed.len());
    assert_eq!(
        fixed
            .iter()
            .map(|v| base16ct::lower::encode_string(&bytes(v)))
            .collect::<Vec<_>>(),
        fixed_php
    );

    let receive = as_ref(invoke(
        &registry,
        STREAM_INIT_PULL,
        vec![
            Value::from(base16ct::mixed::decode_vec(&php_stream[0]).unwrap()),
            key.clone(),
        ],
    ));
    assert_eq!(bytes(&receive).len(), 52);
    let receive_frame = |frame, aad: Vec<u8>| {
        invoke(
            &registry,
            STREAM_PULL,
            vec![receive.clone(), frame, Value::from(aad)],
        )
    };
    let php_frames: Vec<_> = php_stream[1..]
        .iter()
        .map(|v| Value::from(base16ct::mixed::decode_vec(v).unwrap()))
        .collect();
    assert!(matches!(
        receive_frame(php_frames[1].clone(), vec![0x1b; 17]),
        Value::Bool(false)
    ));
    for (i, ((message, aad, tag), frame)) in stream_frames().into_iter().zip(php_frames).enumerate()
    {
        if i == 3 {
            invoke(&registry, STREAM_REKEY, vec![receive.clone()]);
        }
        let unchanged = bytes(&receive);
        let mut shortened = bytes(&frame);
        shortened.pop().unwrap();
        assert!(matches!(
            receive_frame(Value::from(shortened), aad.clone()),
            Value::Bool(false)
        ));
        assert_eq!(bytes(&receive), unchanged);
        let output = receive_frame(frame.clone(), aad.clone());
        assert_eq!(bytes(&index(&output, 0)), message);
        assert!(matches!(index(&output, 1), Value::Int(n) if n == tag));
        assert!(matches!(receive_frame(frame, aad), Value::Bool(false)));
    }

    let pair = invoke(&registry, STREAM_INIT_PUSH, vec![key.clone()]);
    let sender = as_ref(index(&pair, 0));
    assert_eq!(bytes(&sender).len(), 52);
    let frames = encrypt(&registry, &sender);
    let frames_php = frames
        .iter()
        .map(oracle::php_literal)
        .collect::<Vec<_>>()
        .join(", ");
    let open_phera = format!(
        "<?php\n{STREAM_PHP}\n
        $state = {STREAM_INIT_PULL}({}, {});
        $frames = [{frames_php}];
        $cases = stream_cases();
        $copy = $state;
        if ({STREAM_PULL}($state, $frames[1], $cases[1][1]) !== false) exit(1);
        foreach ($cases as $i => [$message, $aad, $tag]) {{
            if ($i === 3) {STREAM_REKEY}($state);
            if ({STREAM_PULL}($state, substr($frames[$i], 0, -1), $aad) !== false) exit(2);
            if ({STREAM_PULL}($state, $frames[$i], $aad . '!') !== false) exit(3);
            if ({STREAM_PULL}($state, $frames[$i], $aad) !== [$message, $tag]) exit(4);
            if ({STREAM_PULL}($state, $frames[$i], $aad) !== false) exit(5);
        }}
        if ({STREAM_PULL}($copy, $frames[0]) !== ['', 0]) exit(6);
        echo \"OK\\n\";
        ",
        oracle::php_literal(&index(&pair, 1)),
        oracle::php_literal(&key),
    );
    assert_eq!(
        oracle::run_php("PHP decrypts Phera secretstream", &open_phera).unwrap(),
        ["OK"]
    );
}

#[test]
fn same_length_state_corruption_is_rejected_without_mutating_the_variable() {
    let registry = registry();
    let hash = invoke(&registry, HASH_INIT, vec![]);
    let stream = index(
        &invoke(&registry, STREAM_INIT_PUSH, vec![Value::from(vec![0; 32])]),
        0,
    );
    for (state, operations) in [
        (
            hash,
            vec![
                (HASH_UPDATE, vec![Value::from("more")]),
                (HASH_FINAL, vec![]),
            ],
        ),
        (
            stream,
            vec![
                (STREAM_PUSH, vec![Value::from("more")]),
                (STREAM_PULL, vec![Value::from("")]),
                (STREAM_REKEY, vec![]),
            ],
        ),
    ] {
        let valid = bytes(&state);
        for i in 0..valid.len() {
            let mut corrupt = valid.clone();
            corrupt[i] ^= 1;
            let state = as_ref(Value::from(corrupt.clone()));
            for (function, tail) in &operations {
                let mut args = vec![state.clone()];
                args.extend(tail.iter().cloned());
                let result = native(&registry, function, args);
                let expected = if *function == STREAM_PUSH {
                    format!(
                        "E:SodiumException:{STREAM_PUSH}(): Argument #1 ($state) must have a correct length"
                    )
                } else {
                    "E:SodiumException:incorrect state length".into()
                };
                assert_eq!(
                    oracle::encode_case("error", &result, b"", &Default::default()),
                    format!("error={expected}")
                );
                assert_eq!(bytes(&state), corrupt);
            }
        }
    }
}
