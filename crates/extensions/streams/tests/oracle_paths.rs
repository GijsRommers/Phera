//! Cross-check path predicates, metadata, binary reads and failure paths on real PHP.

mod oracle_cases;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use oracle_cases::{Case, check};
use phera_extension_api::Value;

const OFFSETS: &[i64] = &[
    -260, -256, -160, -64, -19, -12, -7, -3, -1, 0, 1, 2, 3, 4, 7, 11, 22, 64, 128, 255, 256, 300,
];

fn paths() -> Vec<Value> {
    [
        b"binary".as_slice(),
        b"text",
        b"empty",
        b"no-access",
        b"missing",
        b"sub",
        b"link",
        b"dangling",
        b"./binary",
        b"sub/../binary",
        b"",
        b"bad\0name",
        b"bad\xFFname",
        b"binary/",
        b"file://server/binary",
        b"other://whatever",
        b"php://memory",
        b"data:,xyz",
        b" \t ",
    ]
    .into_iter()
    .map(|path| Value::from(path.to_vec()))
    .collect()
}

fn fixtures(path: &Path) {
    fs::write(path.join("binary"), (0..=255u8).collect::<Vec<_>>()).unwrap();
    fs::write(path.join("text"), b"\nA\r\n\nB\r\n\r\nC\rD").unwrap();
    fs::write(path.join("empty"), b"").unwrap();
    fs::write(path.join("no-access"), b"private").unwrap();
    fs::set_permissions(path.join("no-access"), fs::Permissions::from_mode(0o0)).unwrap();
    fs::create_dir(path.join("sub")).unwrap();
    symlink("binary", path.join("link")).unwrap();
    symlink("missing", path.join("dangling")).unwrap();
}

#[tokio::test]
async fn path_metadata_matches_php() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fixtures(&root);
    let mut cases = Vec::new();
    for function in [
        "file_exists",
        "is_file",
        "is_dir",
        "is_link",
        "is_readable",
        "is_writable",
        "is_writeable",
        "is_executable",
        "filesize",
        "filemtime",
        "fileatime",
        "filectime",
        "fileperms",
        "fileinode",
        "fileowner",
        "filegroup",
        "filetype",
        "stat",
        "lstat",
        "realpath",
        "disk_total_space",
    ] {
        for name in paths() {
            cases.push((function, vec![name]));
        }
        cases.push((
            function,
            vec![Value::from(phera_extension_api::PhpArray::new())],
        ));
    }
    for name in [
        Value::from("missing"),
        Value::from(""),
        Value::from(b"a\0b".as_slice().to_vec()),
    ] {
        cases.push(("disk_free_space", vec![name]));
    }
    cases.extend([
        ("clearstatcache", vec![]),
        (
            "clearstatcache",
            vec![Value::Bool(true), Value::from("binary")],
        ),
        ("getcwd", vec![]),
        ("umask", vec![]),
    ]);
    for name in paths() {
        cases.push(("dirname", vec![name.clone(), Value::Int(2)]));
        cases.push(("pathinfo", vec![name.clone()]));
        cases.push(("pathinfo", vec![name, Value::Int(8)]));
    }
    check("path metadata", &cases, &root, &root, "umask(0022);").await;
}

#[tokio::test]
async fn offset_length_and_line_flags_match_php() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    fixtures(&root);
    let mut cases: Vec<Case> = Vec::new();
    for name in [
        "binary",
        "text",
        "empty",
        "link",
        "dangling",
        "missing",
        "data:,abcd%FF",
        "",
    ] {
        for &offset in OFFSETS {
            for length in [
                Value::Null,
                Value::Int(-1),
                Value::Int(0),
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(7),
                Value::Int(12),
                Value::Int(16),
                Value::Int(31),
                Value::Int(64),
                Value::Int(128),
                Value::Int(257),
            ] {
                for use_path in [false, true] {
                    cases.push((
                        "file_get_contents",
                        vec![
                            Value::from(name),
                            Value::Bool(use_path),
                            Value::Null,
                            Value::Int(offset),
                            length.clone(),
                        ],
                    ));
                }
            }
        }
    }
    for name in paths() {
        for flags in [0, 1, 2, 3, 4, 6, 15, 16, 17, 23, -1] {
            cases.push(("file", vec![name.clone(), Value::Int(flags)]));
        }
        cases.push(("readfile", vec![name.clone()]));
        cases.push(("file_get_contents", vec![name]));
    }
    for (name, length) in [
        ("sub", 0),
        ("sub", 7),
        ("no-access", 8),
        ("binary", i64::MAX),
        ("text", i64::MAX),
        ("empty", i64::MAX),
    ] {
        cases.push((
            "file_get_contents",
            vec![
                Value::from(name),
                Value::Bool(false),
                Value::Null,
                Value::Int(0),
                Value::Int(length),
            ],
        ));
    }
    let before = fs::read(root.join("binary")).unwrap();
    check(
        "whole-file reads",
        &cases,
        &root,
        &root,
        "set_include_path('.');",
    )
    .await;
    assert_eq!(fs::read(root.join("binary")).unwrap(), before);
    assert_eq!(
        fs::read_link(root.join("dangling")).unwrap(),
        Path::new("missing")
    );
}
