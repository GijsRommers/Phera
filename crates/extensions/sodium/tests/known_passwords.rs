//! Password hash vectors from libsodium 1.0.22's `test/default` suite.

use phera_ext_sodium::Sodium;
use phera_extension_api::{
    ClassEntry, NativeFn, Registry, Value, prepare_arguments, testing::TestContext,
};

async fn call(registry: &Registry, name: &str, mut args: Vec<Value>) -> Value {
    let entry = registry.get_function(name).unwrap();
    let mut context = TestContext::default();
    prepare_arguments(entry, &mut args, &mut context).unwrap();
    let NativeFn::Async(handler) = entry.handler else {
        panic!("expected an off-thread password operation");
    };
    handler(&mut context, args).await.unwrap()
}

#[tokio::test]
async fn libsodium_known_password_hashes() {
    let mut registry = Registry::new();
    registry.class(ClassEntry::class("Exception"));
    registry.load(&Sodium);
    // pwhash_argon2i.c / pwhash_argon2id.c, first deterministic fixture.
    let password = hex::decode(concat!(
        "a347ae92bce9f80f6f595a4480fc9c2fe7e7d7148d371e9487d75f5c23008ffae06557",
        "7a928febd9b1973a5a95073acdbeb6a030cfc0d79caa2dc5cd011cef02c08da232d76d",
        "52dfbca38ca8dcbd665b17d1665f7cf5fe59772ec909733b24de97d6f58d220b20c60d",
        "7c07ec1fd93c52c31020300c6c1facd77937a597c7a6",
    ))
    .unwrap();
    let salt = hex::decode("5541fbc995d5c197ba290346d2c559de").unwrap();
    let hashes = [
        (
            1,
            concat!(
                "23b803c84eaa25f4b44634cc1e5e37792c53fcd9b1eb20f865329c68e09cbfa9f19687",
                "57901b383fce221afe27713f97914a041395bbe1fb70e079e5bed2c7145b1f6154046f",
                "5958e9b1b29055454e264d1f2231c316f26be2e3738e83a80315e9a0951ce4b137b52e",
                "7d5ee7b37f7d936dcee51362bcf792595e3c896ad5042734fc90c92cae572ce63ff659",
                "a2f7974a3bd730d04d525d253ccc38",
            ),
        ),
        (
            2,
            concat!(
                "18acec5d6507739f203d1f5d9f1d862f7c2cdac4f19d2bdff64487e60d969e3ced6153",
                "37b9eec6ac4461c6ca07f0939741e57c24d0005c7ea171a0ee1e7348249d135b38f222",
                "e4dad7b9a033ed83f5ca27277393e316582033c74affe2566a2bea47f91f0fd9fe49ec",
                "e7e1f79f3ad6e9b23e0277c8ecc4b313225748dd2a80f5679534a0700e246a79a49b3f",
                "74eb89ec6205fe1eeb941c73b1fcf1",
            ),
        ),
    ];
    for (algorithm, expected) in hashes {
        let hash = call(
            &registry,
            "sodium_crypto_pwhash",
            vec![
                Value::Int(155),
                Value::from(password.as_slice()),
                Value::from(salt.as_slice()),
                Value::Int(5),
                Value::Int(7_256_678),
                Value::Int(algorithm),
            ],
        )
        .await;
        assert_eq!(hash, Value::from(hex::decode(expected).unwrap()));
    }

    // pwhash_scrypt.c, final valid password string from the verification table.
    let hash = concat!(
        "$7$A6....1....TrXs5Zk6s8sWHpQgWDIXTR8kUU3s6Jc3s.DtdS8M2i4$",
        "a4ik5hGDN7foMuHOW.cp.CtX01UyCeO0.JAG.AHPpx5",
    );
    assert_eq!(
        call(
            &registry,
            "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
            vec![Value::from(hash), Value::from("Y0!?iQa9M%5ekffW(`"),]
        )
        .await,
        Value::Bool(true)
    );
}
