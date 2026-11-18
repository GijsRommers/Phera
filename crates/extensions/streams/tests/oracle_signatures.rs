//! Compare error arguments and non-mutating opens against the PHP signatures.

mod oracle_cases;

use oracle_cases::{Case, check};
use phera_extension_api::{PhpArray, Value};

#[tokio::test]
async fn stream_arguments_and_wrapper_openings_match_php() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    std::fs::write(root.join("file"), b"abc").unwrap();
    let no_options = Value::from(PhpArray::new());
    let mut cases: Vec<Case> = Vec::new();
    for (function, tail) in [
        ("fclose", vec![]),
        ("fgetc", vec![]),
        ("fgets", vec![]),
        ("feof", vec![]),
        ("ftell", vec![]),
        ("fflush", vec![]),
        ("rewind", vec![]),
        ("fstat", vec![]),
        ("fsync", vec![]),
        ("fdatasync", vec![]),
        ("fpassthru", vec![]),
        ("get_resource_type", vec![]),
        ("get_resource_id", vec![]),
        ("stream_get_contents", vec![]),
        ("stream_get_meta_data", vec![]),
        ("stream_isatty", vec![]),
        ("stream_context_get_options", vec![]),
        ("stream_set_blocking", vec![Value::Bool(true)]),
        ("stream_set_timeout", vec![Value::Int(1)]),
        ("stream_set_write_buffer", vec![Value::Int(0)]),
        ("stream_set_read_buffer", vec![Value::Int(0)]),
        ("fseek", vec![Value::Int(1)]),
        ("ftruncate", vec![Value::Int(1)]),
        ("fread", vec![Value::Int(1)]),
        ("flock", vec![Value::Int(1)]),
        ("fwrite", vec![Value::from("value")]),
        ("fputs", vec![Value::from("value")]),
        ("fscanf", vec![Value::from("%d")]),
        ("fprintf", vec![Value::from("%d"), Value::Int(1)]),
        ("vfprintf", vec![Value::from("%d"), no_options.clone()]),
        ("fgetcsv", vec![]),
        ("fputcsv", vec![no_options.clone()]),
        ("readdir", vec![]),
        ("rewinddir", vec![]),
        ("closedir", vec![]),
        ("stream_copy_to_stream", vec![Value::Bool(false)]),
    ] {
        for bad in [
            Value::Bool(false),
            Value::Int(7),
            Value::from("wrong"),
            no_options.clone(),
        ] {
            let mut arguments = vec![bad];
            arguments.extend_from_slice(&tail);
            cases.push((function, arguments));
        }
    }
    for path in [
        "file",
        "absent",
        "php://memory",
        "php://temp",
        "data:,x",
        "data:bad,x",
        "",
    ] {
        for mode in ["r", "rb", "rt", "r+e", "rnb", "", "t", "q+"] {
            cases.push(("fopen", vec![Value::from(path), Value::from(mode)]));
        }
    }
    for (pattern, name) in [
        ("*.php", "file.php"),
        ("*.php", "dir/file.php"),
        ("*", ".hidden"),
        (".*", ".hidden"),
        ("[[:lower:]]", "a"),
        ("[!0-9]", "7"),
        ("f\\*", "f*"),
        ("data?", "data"),
        ("[abc", "[abc"),
        ("", ""),
    ] {
        for flags in [0, 1, 2, 4, 16] {
            cases.push((
                "fnmatch",
                vec![Value::from(pattern), Value::from(name), Value::Int(flags)],
            ));
        }
    }
    for path in ["file", "absent", "", "file\0x", "not-a-wrapper://path"] {
        for function in [
            "scandir",
            "opendir",
            "dir",
            "stream_resolve_include_path",
            "chdir",
        ] {
            cases.push((function, vec![Value::from(path)]));
        }
    }
    for filename in ["a", "*", "file", "f?le", "???e", "[fg]*", "nomatch*"] {
        cases.push(("glob", vec![Value::from(filename)]));
    }
    check(
        "resource arguments and wrapper opening",
        &cases,
        root,
        root,
        "",
    )
    .await;
}
