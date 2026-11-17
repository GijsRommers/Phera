//! Local file hashes use async natives; diagnostics are compared with the same filesystem.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, NativeFn, NativeResult, Registry, Value, prepare_arguments};
async fn invoke(
    r: &Registry,
    c: &mut TestContext,
    name: &str,
    mut args: Vec<Value>,
) -> NativeResult {
    let e = r.get_function(name).unwrap();
    prepare_arguments(e, &mut args, c)?;
    match e.handler {
        NativeFn::Sync(f) => f(c, &args),
        NativeFn::Async(f) => f(c, args).await,
    }
}
#[tokio::test]
async fn files_and_incremental_file() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    let dir = std::env::temp_dir().join(format!("phera-hash-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("input");
    std::fs::write(&path, vec![0xff; 20001]).unwrap();
    let filename = path.to_string_lossy().to_string();
    let missing = dir.join("missing").to_string_lossy().to_string();
    let mut script = format!("<?php {}", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    let mut labels = vec![];
    for name in ["hash_file", "hash_hmac_file", "md5_file", "sha1_file"] {
        for file in [&filename, &missing, "", "a\0b"] {
            for binary in [false, true] {
                context.diagnostics.entries.clear();
                let mut args = vec![];
                if name.starts_with("hash") {
                    args.push(Value::from("sha256"));
                }
                args.push(Value::from(file));
                if name == "hash_hmac_file" {
                    args.push(Value::from("key"));
                }
                args.push(Value::Bool(binary));
                let expression = format!(
                    "{name}({})",
                    args.iter()
                        .map(oracle::php_literal)
                        .collect::<Vec<_>>()
                        .join(",")
                );
                script.push_str(&format!("run('file',fn()=>{expression});\n"));
                let result = invoke(&registry, &mut context, name, args).await;
                rust.push(oracle::encode_case(
                    "file",
                    &result,
                    &context.output,
                    &context.diagnostics,
                ));
                labels.push(expression);
            }
        }
    }
    for algorithm in ["sha256", "haval128,3", "snefru", "xxh3"] {
        for flags in [0, 1] {
            if algorithm == "xxh3" && flags == 1 {
                continue;
            }
            let hash = call(
                &registry,
                &mut context,
                "hash_init",
                vec![
                    Value::from(algorithm),
                    Value::Int(flags),
                    Value::from("key"),
                ],
            )
            .unwrap();
            let result = invoke(
                &registry,
                &mut context,
                "hash_update_file",
                vec![hash.clone(), Value::from(filename.clone())],
            )
            .await
            .unwrap();
            assert_eq!(result, Value::Bool(true));
            let result = call(&registry, &mut context, "hash_final", vec![hash]);
            context.diagnostics.entries.clear();
            script.push_str(&format!("run('file',function(){{$h=hash_init('{algorithm}',{flags},'key');hash_update_file($h,{});return hash_final($h);}});",oracle::php_literal(&Value::from(filename.clone()))));
            rust.push(oracle::encode_case(
                "file",
                &result,
                &context.output,
                &context.diagnostics,
            ));
            labels.push(algorithm.into());
        }
    }
    let php = oracle::run_php("hash-files", &script).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    oracle::compare("async files", &labels, &php, &rust);
}
