//! Time a normal project-sized environment and exercise deterministic resource bounds.

use std::path::Path;
use std::time::Instant;

use phera_dotenv::{Environment, parse};

#[test]
fn ten_thousand_assignments_and_pathological_values() {
    let empty = Environment::default();
    let read = |bytes: &[u8]| parse(Path::new(".env"), bytes, &empty, &empty);
    let bytes: String = (0..10_000).map(|i| format!("VAR_{i}=value{i}\n")).collect();
    let start = Instant::now();
    assert_eq!(read(bytes.as_bytes()).unwrap().iter().count(), 10_000);
    eprintln!("10,000 assignments in {:?}", start.elapsed());
    let huge = format!("A={}", "x".repeat(10_000_000));
    assert_eq!(
        read(huge.as_bytes()).err().unwrap().expected,
        "a value of at most 8 MiB"
    );
    let nested = format!("A={}x{}", "${A:-".repeat(1_000_000), "}".repeat(1_000_000));
    assert_eq!(
        read(nested.as_bytes()).err().unwrap().expected,
        "a literal default followed by '}'"
    );
    let cycle = read(b"A=$B\nB=$A").unwrap();
    assert_eq!(cycle.get(b"A"), Some(b"".as_slice()));
    assert_eq!(cycle.get(b"B"), Some(b"".as_slice()));
}
