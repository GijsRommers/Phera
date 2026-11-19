//! Known-answer data from libsodium 1.0.22's `test/default` suite.

use phera_ext_sodium::Sodium;
use phera_extension_api::{
    ClassEntry, Registry, Value,
    testing::{TestContext, call},
};

fn bytes(hexadecimal: &str) -> Value {
    Value::from(base16ct::mixed::decode_vec(hexadecimal).unwrap())
}

fn invoke(registry: &Registry, name: &str, args: Vec<Value>) -> Value {
    call(registry, &mut TestContext::default(), name, args).unwrap()
}

#[test]
fn libsodium_known_answers() {
    let mut registry = Registry::new();
    registry.class(ClassEntry::class("Exception"));
    registry.load(&Sodium);
    let sequence = |size: u8| Value::from((0..size).collect::<Vec<_>>());
    let cases = [
        // secretbox.c / secretbox.exp, excluding the legacy leading zero padding.
        (
            "sodium_crypto_secretbox",
            vec![
                bytes(concat!(
                    "be075fc53c81f2d5cf141316ebeb0c7b5228c52a4c62cbd44b66849b64244ffce5ecba",
                    "af33bd751a1ac728d45e6c61296cdc3c01233561f41db66cce314adb310e3be8250c46",
                    "f06dceea3a7fa1348057e2f6556ad6b1318a024a838f21af1fde048977eb48f59ffd49",
                    "24ca1c60902e52f0a089bc76897040e082f937763848645e0705",
                )),
                bytes("69696ee955b62b73cd62bda875fc73d68219e0036b7a0b37"),
                bytes("1b27556473e985d462cd51197a9a46c76009549eac6474f206c4ee0844f68389"),
            ],
            concat!(
                "f3ffc7703f9400e52a7dfb4b3d3305d98e993b9f48681273c29650ba32fc76ce48332e",
                "a7164d96a4476fb8c531a1186ac0dfc17c98dce87b4da7f011ec48c97271d2c20f9b92",
                "8fe2270d6fb863d51738b48eeee314a7cc8ab932164548e526ae90224368517acfeabd",
                "6bb3732bc0e9da99832b61ca01b6de56244a9e88d5f9b37973f622a43d14a6599b1f65",
                "4cb45a74e355a5",
            ),
        ),
        // box.c / box.exp
        (
            "sodium_crypto_box",
            vec![
                bytes(concat!(
                    "be075fc53c81f2d5cf141316ebeb0c7b5228c52a4c62cbd44b66849b64244ffce5ecba",
                    "af33bd751a1ac728d45e6c61296cdc3c01233561f41db66cce314adb310e3be8250c46",
                    "f06dceea3a7fa1348057e2f6556ad6b1318a024a838f21af1fde048977eb48f59ffd49",
                    "24ca1c60902e52f0a089bc76897040e082f937763848645e0705",
                )),
                bytes("69696ee955b62b73cd62bda875fc73d68219e0036b7a0b37"),
                bytes(concat!(
                    "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2ade9edb",
                    "7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f",
                )),
            ],
            concat!(
                "f3ffc7703f9400e52a7dfb4b3d3305d98e993b9f48681273c29650ba32fc76ce48332e",
                "a7164d96a4476fb8c531a1186ac0dfc17c98dce87b4da7f011ec48c97271d2c20f9b92",
                "8fe2270d6fb863d51738b48eeee314a7cc8ab932164548e526ae90224368517acfeabd",
                "6bb3732bc0e9da99832b61ca01b6de56244a9e88d5f9b37973f622a43d14a6599b1f65",
                "4cb45a74e355a5",
            ),
        ),
        // aead_chacha20poly1305.c / aead_chacha20poly1305.exp
        (
            "sodium_crypto_aead_chacha20poly1305_encrypt",
            vec![
                bytes("86d09974840bded2a5ca"),
                bytes("87e229d4500845a079c0"),
                bytes("cd7cf67be39c794a"),
                bytes("4290bcb154173531f314af57f3be3b5006da371ece272afa1b5dbdd1100a1007"),
            ],
            "e3e446f7ede9a19b62a4677dabf4e3d24b876bb284753896e1d6",
        ),
        // aead_chacha20poly13052.c, RFC 8439
        (
            "sodium_crypto_aead_chacha20poly1305_ietf_encrypt",
            vec![
                bytes(concat!(
                    "4c616469657320616e642047656e746c656d656e206f662074686520636c617373206f",
                    "66202739393a204966204920636f756c64206f6666657220796f75206f6e6c79206f6e",
                    "652074697020666f7220746865206675747572652c2073756e73637265656e20776f75",
                    "6c642062652069742e",
                )),
                bytes("50515253c0c1c2c3c4c5c6c7"),
                bytes("070000004041424344454647"),
                bytes("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f"),
            ],
            concat!(
                "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea4",
                "5e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d77",
                "8b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de5",
                "76d26586cec64b61161ae10b594f09e26a7e902ecbd0600691",
            ),
        ),
        // aead_xchacha20poly1305.c / aead_xchacha20poly1305.exp
        (
            "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
            vec![
                bytes(concat!(
                    "4c616469657320616e642047656e746c656d656e206f662074686520636c617373206f",
                    "66202739393a204966204920636f756c64206f6666657220796f75206f6e6c79206f6e",
                    "652074697020666f7220746865206675747572652c2073756e73637265656e20776f75",
                    "6c642062652069742e",
                )),
                bytes("50515253c0c1c2c3c4c5c6c7"),
                bytes("07000000404142434445464748494a4b4c4d4e4f50515253"),
                bytes("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f"),
            ],
            concat!(
                "f8ebea4875044066fc162a0604e171feecfb3d20425248563bcfd5a155dcc47bbda70b",
                "86e5ab9b55002bd1274c02db35321acd7af8b2e2d25015e136b7679458e9f43243bf71",
                "9d639badb5feac03f80a19a96ef10cb1d15333a837b90946ba3854ee74da3f2585efc7",
                "e1e170e17e15e563e77601f4f85cafa8e5877614e143e68420",
            ),
        ),
        // aead_aes256gcm.c, first test
        (
            "sodium_crypto_aead_aes256gcm_encrypt",
            vec![
                bytes(""),
                bytes(""),
                bytes("516c33929df5a3284ff463d7"),
                bytes("b52c505a37d78eda5dd34f20c22540ea1b58963cf8e5bf8ffa85f9f2492505b4"),
            ],
            "bdc1ac884d332457a1d2664f168c76f0",
        ),
        // shorthash.c / shorthash.exp, zero and seven bytes
        (
            "sodium_crypto_shorthash",
            vec![sequence(0), sequence(16)],
            "310e0edd47db6f72",
        ),
        (
            "sodium_crypto_shorthash",
            vec![sequence(7), sequence(16)],
            "37d1018bf50002ab",
        ),
        // auth.c / auth.exp, first HMAC-SHA-512-256 tag
        (
            "sodium_crypto_auth",
            vec![
                Value::from("what do ya want for nothing?"),
                bytes("4a65666500000000000000000000000000000000000000000000000000000000"),
            ],
            "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea250554",
        ),
        // kdf.c / kdf.exp, first subkey
        (
            "sodium_crypto_kdf_derive_from_key",
            vec![
                Value::Int(64),
                Value::Int(0),
                Value::from("KDF test"),
                sequence(32),
            ],
            concat!(
                "a0c724404728c8bb95e5433eb6a9716171144d61efb23e74b873fcbeda51d8071b5d70",
                "aae12066dfc94ce943f145aa176c055040c3dd73b0a15e36254d450614",
            ),
        ),
        // kx.c / kx.exp, client keypair from its deterministic seed
        (
            "sodium_crypto_kx_seed_keypair",
            vec![sequence(32)],
            concat!(
                "cb2f5160fc1f7e05a55ef49d340b48da2e5a78099d53393351cd579dd42503d60e0216",
                "223f147143d32615a91189c288c1728cba3cc5f9f621b1026e03d83129",
            ),
        ),
        // sign.c, RFC 8032 test 1
        (
            "sodium_crypto_sign_detached",
            vec![
                bytes(""),
                bytes(concat!(
                    "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60d75a98",
                    "0182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                )),
            ],
            concat!(
                "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb882",
                "1590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            ),
        ),
        // generichash.c, keyed empty message
        (
            "sodium_crypto_generichash",
            vec![sequence(0), sequence(64), Value::Int(64)],
            concat!(
                "10ebb67700b1868efb4417987acf4690ae9d972fb7a590c2f02871799aaa4786b5e996",
                "e8f0f4eb981fc214b005f42d2ff4233499391653df7aefcbc13fc51568",
            ),
        ),
        // stream.c / stream.exp, the 63-byte XSalsa20 keystream
        (
            "sodium_crypto_stream",
            vec![
                Value::Int(63),
                bytes("69696ee955b62b73cd62bda875fc73d68219e0036b7a0b37"),
                bytes("1b27556473e985d462cd51197a9a46c76009549eac6474f206c4ee0844f68389"),
            ],
            concat!(
                "eea6a7251c1e72916d11c2cb214d3c252539121d8e234e652d651fa4c8cff880309e64",
                "5a74e9e0a60d8243acd9177ab51a1beb8d5a2f5d700c093c5e558557",
            ),
        ),
        // xchacha20.c, first keystream
        (
            "sodium_crypto_stream_xchacha20",
            vec![
                Value::Int(29),
                bytes("b33ffd3096479bcfbc9aee49417688a0a2554f8d95389419"),
                bytes("79c99798ac67300bbb2704c95c341e3245f3dcb21761b98e52ff45b24f304fc4"),
            ],
            "c6e9758160083ac604ef90e712ce6e75d7797590744e0cf060f013739c",
        ),
        // core_ristretto255.c / core_ristretto255.exp, first uniform map
        (
            "sodium_crypto_core_ristretto255_from_hash",
            vec![bytes(concat!(
                "5d1be09e3d0c82fc538112490e35701979d99e06ca3e2b5b54bffe8b4dc772c14d98b6",
                "96a1bbfb5ca32c436cc61c16563790306c79eaca7705668b47dffe5bb6",
            ))],
            "3066f82a1a747d45120d1740f14358531a8f04bbffe6a819f86dfe50f44a0a46",
        ),
    ];
    for (name, args, expected) in cases {
        let actual = invoke(&registry, name, args);
        assert_eq!(actual, bytes(expected), "{name}");
    }
}
