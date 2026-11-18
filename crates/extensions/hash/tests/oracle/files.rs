//! The `*_file` functions on a real file, a missing one, an empty path and a
//! path with a NUL byte; PHP reads the same files.

use phera_extension_api::Value;
use phera_extension_api::testing::{TestContext, call, oracle};

use crate::support::{Comparison, invoke, php_call, registry};

#[tokio::test]
async fn files_and_incremental_file() {
    let registry = registry();
    let mut context = TestContext::default();
    let directory = std::env::temp_dir().join(format!("phera-hash-files-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("input");
    std::fs::write(&path, vec![0xff; 20001]).unwrap();
    let filename = path.to_string_lossy().to_string();
    let missing = directory.join("missing").to_string_lossy().to_string();
    let mut comparison = Comparison::new();
    for name in ["hash_file", "hash_hmac_file", "md5_file", "sha1_file"] {
        for file in [&filename, &missing, "", "a\0b"] {
            for binary in [false, true] {
                let mut arguments = vec![];
                if name.starts_with("hash") {
                    arguments.push(Value::from("sha256"));
                }
                arguments.push(Value::from(file));
                if name == "hash_hmac_file" {
                    arguments.push(Value::from("key"));
                }
                arguments.push(Value::Bool(binary));
                let expression = php_call(name, &arguments);
                let result = invoke(&registry, &mut context, name, arguments).await;
                comparison.record_expression(&expression, &result, &mut context);
            }
        }
    }
    for algorithm in ["sha256", "haval128,3", "snefru", "xxh3"] {
        // xxh3 is not cryptographic, so it has no HMAC.
        let flags: &[i64] = if algorithm == "xxh3" { &[0] } else { &[0, 1] };
        for &flags in flags {
            let init = vec![
                Value::from(algorithm),
                Value::Int(flags),
                Value::from("key"),
            ];
            let hash = call(&registry, &mut context, "hash_init", init).unwrap();
            let update = vec![hash.clone(), Value::from(filename.clone())];
            let updated = invoke(&registry, &mut context, "hash_update_file", update).await;
            assert_eq!(updated.unwrap(), Value::Bool(true));
            let result = call(&registry, &mut context, "hash_final", vec![hash]);
            let closure = format!(
                "function () {{ $h = hash_init('{algorithm}', {flags}, 'key'); \
                 hash_update_file($h, {}); return hash_final($h); }}",
                oracle::php_literal(&Value::from(filename.clone()))
            );
            comparison.record(&closure, &result, &mut context);
        }
    }
    comparison.compare("hash-files", "async files");
    std::fs::remove_dir_all(directory).unwrap();
}
