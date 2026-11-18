//! Exercise the byte reader with arbitrary bytes and mutations of native PHP encodings.

use phera_ext_serialize::Serialize;
use phera_ext_serialize::wire::{ObjectFormat, ParseError, Reader, Token};
use phera_extension_api::{
    Registry, Value,
    testing::{TestContext, call},
};

fn walk(bytes: &[u8]) -> Result<(), ParseError> {
    let mut reader = Reader::new(bytes);
    let mut frames = Vec::new();
    loop {
        if !frames.is_empty() && !matches!(reader.token(true)?, Token::Int(_) | Token::String(_)) {
            return Err(ParseError);
        }
        let count = match reader.token(false)? {
            Token::Array(count) => Some(count),
            Token::Object(_, ObjectFormat::Properties) => Some(reader.properties()?),
            Token::Object(_, ObjectFormat::Custom) => {
                reader.custom_data()?;
                None
            }
            _ => None,
        };
        match count {
            Some(0) => reader.close()?,
            Some(count) => {
                if frames.len() >= 4096 {
                    return Err(ParseError);
                }
                frames.push(count);
                continue;
            }
            None => {}
        }
        loop {
            let Some(remaining) = frames.last_mut() else {
                return Ok(());
            };
            *remaining -= 1;
            if *remaining != 0 {
                break;
            }
            frames.pop();
            reader.close()?;
        }
    }
}

#[test]
fn arbitrary_and_mutated_bytes_return() {
    let mut state = 0xf3d1235b581fcb3cu64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let seeds: &[&[u8]] = &[
        b"a:2:{i:0;a:1:{s:1:\"a\";d:-1.5E-5;}i:1;R:1;}",
        b"O:8:\"stdClass\":1:{s:1:\"a\";s:4:\"x\0\xffy\";}",
        b"a:2:{i:0;C:1:\"X\":3:{a\0b}i:1;r:2;}",
        b"a:1:{S:2:\"\\00a\";E:5:\"Foo:A\";}",
        b"a:2000000000:{i:0;N;}",
        b"s:18446744073709551615:\"a\";",
    ];
    let mut input = Vec::new();
    let mut registry = Registry::new();
    registry.load(&Serialize);
    for index in 0..300_000 {
        input.clear();
        if index % 2 == 0 {
            input.extend((0..next() % 128).map(|_| next() as u8));
        } else {
            input.extend_from_slice(seeds[next() as usize % seeds.len()]);
            for _ in 0..next() % 5 {
                let at = next() as usize % (input.len() + 1);
                match next() % 3 {
                    0 => input.insert(at, next() as u8),
                    1 => input.truncate(at),
                    _ if at < input.len() => input[at] = next() as u8,
                    _ => {}
                }
            }
        }
        let _ = walk(&input);
        let _ = call(
            &registry,
            &mut TestContext::default(),
            "unserialize",
            vec![Value::from(input.clone())],
        );
    }
}

#[test]
fn deep_values_use_heap_frames_with_or_without_a_depth_limit() {
    use phera_extension_api::{ArrayKey, PhpArray};

    let depth = 20_000;
    let mut input = Vec::new();
    for _ in 0..depth {
        input.extend_from_slice(b"a:1:{i:0;");
    }
    input.extend_from_slice(b"N;");
    input.resize(input.len() + depth, b'}');
    let input = Value::from(input);
    let mut registry = Registry::new();
    registry.load(&Serialize);
    let mut context = TestContext::default();
    assert_eq!(
        call(&registry, &mut context, "unserialize", vec![input.clone()]).unwrap(),
        Value::Bool(false)
    );
    let options: PhpArray = [(ArrayKey::str("max_depth"), Value::Int(0))]
        .into_iter()
        .collect();
    let value = call(
        &registry,
        &mut context,
        "unserialize",
        vec![input.clone(), options.into()],
    )
    .unwrap();
    assert_eq!(
        call(&registry, &mut context, "serialize", vec![value]).unwrap(),
        input
    );
}
