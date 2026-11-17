//! Static reference vectors from php-src/ext/hash/tests (see the TSV source column).
//! These expectations include RFC 1320 and FIPS-202 vectors and never invoke PHP.
//! Copyright The PHP Group; BSD-3-Clause, see ../LICENSE-PHP.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call};
use phera_extension_api::{Extension, Registry, Value};
#[test]
fn published_vectors() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    for line in include_str!("published-vectors.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let data = hex::decode(fields[2]).unwrap();
        let result = call(
            &registry,
            &mut context,
            "hash",
            vec![Value::from(fields[1]), Value::from(data)],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::from(fields[3]),
            "{} {}",
            fields[0],
            fields[1]
        );
    }
}

#[test]
fn published_seeded_xxhash_vectors() {
    use phera_extension_api::{ArrayKey, PhpArray};
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    // php-src/ext/hash/tests/xxhash_seed.phpt, reference xxHash implementation.
    for (algorithm, expected) in [
        ("xxh32", "3d0cc7e5"),
        ("xxh64", "9c9aa071b5d22a15"),
        ("xxh3", "366409913c16b70d"),
        ("xxh128", "f87856a7589354e92aeca886c71ed7fb"),
    ] {
        let mut options = PhpArray::new();
        options.insert(ArrayKey::str("seed"), Value::Int(42));
        let result = call(
            &registry,
            &mut context,
            "hash",
            vec![
                Value::from(algorithm),
                Value::from("Lorem ipsum dolor sit amet, consectetur adipiscing elit."),
                Value::Bool(false),
                Value::from(options),
            ],
        )
        .unwrap();
        assert_eq!(result, Value::from(expected));
    }
}

#[test]
fn published_four_pass_tiger_kdf_vectors() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    // php-src/ext/hash/tests/hash_hkdf_basic.phpt covers the four-pass variants.
    for (algorithm, expected) in [
        ("tiger128,4", "8acf517ecf58cccbd65c1186d71e4116"),
        ("tiger160,4", "cc0e33ee26700a2eb9a994bbb0e6cef29b429441"),
        (
            "tiger192,4",
            "97fa02d42331321fdc05c7f8dbc756d751ca36ce1aee69b0",
        ),
    ] {
        let result = call(
            &registry,
            &mut context,
            "hash_hkdf",
            vec![Value::from(algorithm), Value::from("input key material")],
        )
        .unwrap();
        assert_eq!(result, Value::from(hex::decode(expected).unwrap()));
    }
}
