//! Compare mutation results, diagnostics and the files each run leaves behind.

mod oracle_cases;

use std::fs::{self, Metadata};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use oracle_cases::{Case, check};
use phera_extension_api::{PhpArray, Value};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    path: PathBuf,
    mode: u32,
    size: u64,
    link: Option<PathBuf>,
    content: Option<Vec<u8>>,
}

fn snapshot(path: &Path) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut pending = vec![path.to_owned()];
    while let Some(current) = pending.pop() {
        for entry in current.read_dir().unwrap() {
            let file = entry.unwrap().path();
            let metadata = file.symlink_metadata().unwrap();
            let link = metadata.is_symlink().then(|| file.read_link().unwrap());
            let content = metadata.is_file().then(|| fs::read(&file).unwrap());
            entries.push(Entry {
                path: file.strip_prefix(path).unwrap().to_owned(),
                mode: metadata.permissions().mode(),
                size: if metadata.is_dir() {
                    0
                } else {
                    Metadata::len(&metadata)
                },
                link,
                content,
            });
            if metadata.is_dir() {
                pending.push(file);
            }
        }
    }
    entries.sort();
    entries
}

#[tokio::test]
async fn writes_and_filesystem_state_match_php() {
    let oracle = tempfile::tempdir().unwrap();
    let actual = tempfile::tempdir().unwrap();
    for root in [oracle.path(), actual.path()] {
        fs::write(root.join("target"), b"initial").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        std::os::unix::fs::symlink("missing", root.join("dangling")).unwrap();
    }
    let mut array = PhpArray::new();
    array.push(Value::from("part")).unwrap();
    array.push(Value::Int(5)).unwrap();
    array.push(Value::from(vec![0, 255])).unwrap();
    let pieces = Value::from(array);
    let mut cases: Vec<Case> = Vec::new();
    for filename in [
        "target",
        "created",
        "sub/one",
        "missing/file",
        "",
        "x\0y",
        "dangling",
    ] {
        for flags in [0, 1, 2, 8, 9, 10, 16, 18, 24, 255] {
            for data in [
                Value::Null,
                Value::Bool(false),
                Value::Int(17),
                Value::from(vec![0, 255, 10, 128]),
                pieces.clone(),
            ] {
                cases.push((
                    "file_put_contents",
                    vec![Value::from(filename), data, Value::Int(flags)],
                ));
            }
        }
    }
    for (function, from, to) in [
        ("copy", "target", "copy"),
        ("copy", "sub", "copy"),
        ("copy", "target", "target"),
        ("copy", "absent", "copy"),
        ("rename", "copy", "new"),
        ("rename", "absent", "new"),
        ("symlink", "absent", "new-link"),
        ("symlink", "target", "new-link"),
        ("link", "target", "hard"),
        ("link", "absent", "hard"),
    ] {
        cases.push((function, vec![Value::from(from), Value::from(to)]));
    }
    cases.extend([
        (
            "mkdir",
            vec![Value::from("a/b/c"), Value::Int(0o751), Value::Bool(true)],
        ),
        ("mkdir", vec![Value::from("a/b/c")]),
        ("chmod", vec![Value::from("target"), Value::Int(0o640)]),
        (
            "touch",
            vec![
                Value::from("touch"),
                Value::Int(1_700_000_000),
                Value::Int(1_700_000_001),
            ],
        ),
        ("readlink", vec![Value::from("new-link")]),
        ("unlink", vec![Value::from("new")]),
        ("unlink", vec![Value::from("no-file")]),
        ("rmdir", vec![Value::from("a/b/c")]),
        ("rmdir", vec![Value::from("no-dir")]),
        (
            "chown",
            vec![Value::from("missing/nonexistent"), Value::Int(1)],
        ),
        (
            "chgrp",
            vec![
                Value::from("target"),
                Value::from("phera_nonexistent_group_100"),
            ],
        ),
        (
            "lchown",
            vec![Value::from("missing/nonexistent"), Value::Int(1)],
        ),
        (
            "lchgrp",
            vec![Value::from("missing/nonexistent"), Value::Int(1)],
        ),
    ]);
    check(
        "mutations",
        &cases,
        oracle.path(),
        actual.path(),
        "umask(0022);",
    )
    .await;
    assert_eq!(snapshot(oracle.path()), snapshot(actual.path()));
}
