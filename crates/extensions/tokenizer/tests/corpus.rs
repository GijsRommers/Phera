//! Compare both public tokenizers, with and without `TOKEN_PARSE`, to real PHP.

mod snippets;
mod support;

use std::{fs, path::Path, str};

use phera_extension_api::{Value, testing::oracle};

use snippets::SNIPPETS;
use support::Matrix;

#[test]
fn full_streams_match_php() {
    let root = phera_phpt::tests_dir();
    let mut files = 0;
    let mut matrix = Matrix::new();
    for file in phera_phpt::find_phpt_files(&root) {
        let bytes = fs::read(&file).unwrap();
        if let Some(source) = file_section(&bytes) {
            let label = file.strip_prefix(&root).unwrap().display().to_string();
            compare_source(&mut matrix, label, source);
            files += 1;
        }
    }
    for file in php_files(&root) {
        let label = file.strip_prefix(&root).unwrap().display().to_string();
        compare_source(&mut matrix, label, &fs::read(file).unwrap());
        files += 1;
    }
    for (index, source) in SNIPPETS.iter().enumerate() {
        compare_source(&mut matrix, format!("snippet {index} {source:?}"), source);
    }
    assert!(files >= 300 && SNIPPETS.len() >= 150);
    let expected = oracle::run_php("tokenizer-corpus", &matrix.php).expect("PHP 8.5 required");
    assert_eq!(matrix.labels.len(), expected.len());
    let mut mismatches = 0;
    let mut raw = 0;
    for (index, (expected, actual)) in expected.iter().zip(&matrix.results).enumerate() {
        if expected != actual {
            mismatches += 1;
            raw += usize::from(index % 4 < 2);
            let offset = expected
                .bytes()
                .zip(actual.bytes())
                .take_while(|(left, right)| left == right)
                .count();
            eprintln!(
                "{}\n  first difference at {offset}\n  PHP: {}\n  Phera: {}",
                matrix.labels[index],
                excerpt(expected, offset),
                excerpt(actual, offset),
            );
        }
    }
    println!(
        "{files} files, {} snippets, {} calls, {} Phera tokens, {mismatches} mismatches ({raw} raw)",
        SNIPPETS.len(),
        matrix.labels.len(),
        matrix.token_count
    );
    assert_eq!(mismatches, 0);
}

fn excerpt(line: &str, offset: usize) -> &str {
    let start = line.floor_char_boundary(offset.saturating_sub(25));
    let end = line.ceil_char_boundary((offset + 180).min(line.len()));
    &line[start..end]
}

fn compare_source(matrix: &mut Matrix, label: String, source: &[u8]) {
    for flags in [0, 1] {
        for name in ["token_get_all", "PhpToken::tokenize"] {
            matrix.check(
                format!("{label}, {name}, flags={flags}"),
                name,
                vec![Value::from(source), Value::Int(flags)],
            );
        }
    }
}

fn file_section(phpt: &[u8]) -> Option<&[u8]> {
    let mut lines = phpt.split_inclusive(|&byte| byte == b'\n');
    let mut start = 0;
    for line in &mut lines {
        start += line.len();
        if line.trim_ascii_end() == b"--FILE--" {
            break;
        }
    }
    let mut end = start;
    for line in lines {
        let line_content = line.trim_ascii_end();
        if let Some(name) = line_content
            .strip_prefix(b"--")
            .and_then(|line| line.strip_suffix(b"--"))
            && !name.is_empty()
            && name.iter().all(|b| b.is_ascii_uppercase() || *b == b'_')
        {
            break;
        }
        end += line.len();
    }
    (start < end).then(|| &phpt[start..end])
}

fn php_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "php") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
