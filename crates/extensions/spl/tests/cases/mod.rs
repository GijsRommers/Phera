//! The cases build fresh PHP objects so statistics identify the failing operation.

mod arrays;
mod containers;
mod files;
mod iterators;

pub type Cases = Vec<(String, String, String)>;

pub const HOSTILE: &[&str] = &[
    "null",
    "false",
    "true",
    "0",
    "-1",
    "1",
    "2",
    "PHP_INT_MAX",
    "PHP_INT_MIN",
    "1.5",
    "-0.0",
    "INF",
    "NAN",
    r#""""#,
    "'0'",
    "'01'",
    "'a'",
    r#""a\0b""#,
    r#""\xff""#,
    "[]",
    "[1]",
    "new stdClass",
];

pub fn push(cases: &mut Cases, label: &str, setup: &str, expression: &str) {
    cases.push((
        format!("{label}-{}", cases.len()),
        setup.to_owned(),
        expression.to_owned(),
    ));
}

pub fn methods(cases: &mut Cases, setup: &str, calls: &[&str]) {
    for method in calls {
        push(cases, "method", setup, method);
    }
}

pub fn matrix() -> [(&'static str, Cases); 23] {
    [
        ("ArrayObject", arrays::container("ArrayObject")),
        ("ArrayIterator", arrays::container("ArrayIterator")),
        ("RecursiveArrayIterator", arrays::recursive()),
        ("SplObjectStorage", containers::storage()),
        ("WeakReference", containers::weak_reference()),
        ("WeakMap", containers::weak_map()),
        (
            "SplDoublyLinkedList",
            containers::linked("SplDoublyLinkedList"),
        ),
        ("SplQueue", containers::linked("SplQueue")),
        ("SplStack", containers::linked("SplStack")),
        ("SplFixedArray", containers::fixed()),
        ("SplMinHeap", containers::heap("SplMinHeap")),
        ("SplMaxHeap", containers::heap("SplMaxHeap")),
        ("SplPriorityQueue", containers::heap("SplPriorityQueue")),
        ("iterator_wrappers", iterators::wrappers()),
        ("recursive_wrappers", iterators::recursive()),
        ("SPL_protocols", iterators::protocols()),
        ("SplFileInfo", files::info()),
        ("SplFileObject", files::stream(false)),
        ("SplTempFileObject", files::stream(true)),
        ("DirectoryIterator", files::directory("DirectoryIterator")),
        ("FilesystemIterator", files::directory("FilesystemIterator")),
        (
            "RecursiveDirectoryIterator",
            files::directory("RecursiveDirectoryIterator"),
        ),
        ("GlobIterator", files::directory("GlobIterator")),
    ]
}
