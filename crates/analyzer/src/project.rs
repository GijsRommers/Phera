//! File discovery and parallel parsing: checking `src/` skips its `vendor/` tree.
//! Each worker owns a parser-sized stack; results are sorted after joining.
//! PHP files such as `src/a.php` containing `new Missing` become parsed inputs.

use crate::diagnostic::Diagnostic;
use phera_ast::Program;
use phera_source::LineTable;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct ParsedFile {
    pub path: String,
    pub source: String,
    pub lines: LineTable,
    pub program: Program,
}

pub fn discover(paths: &[String], excluded: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut files = std::collections::BTreeSet::new();
    let working_directory = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .map_err(|e| e.to_string())?;
    for path in paths {
        let root = PathBuf::from(path);
        if !root.exists() {
            return Err(format!("Path does not exist: {path}"));
        }
        let explicit_vendor = root.components().any(|c| c.as_os_str() == "vendor");
        let mut pending = vec![root];
        while let Some(path) = pending.pop() {
            let name = display_path(&path);
            let relative = if excluded.is_empty() {
                name.clone()
            } else {
                path.canonicalize()
                    .map(|p| display_path(p.strip_prefix(&working_directory).unwrap_or(&p)))
                    .unwrap_or_else(|_| name.clone())
            };
            if excluded
                .iter()
                .any(|pattern| glob(pattern, &name) || glob(pattern, &relative))
            {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if !explicit_vendor && path.file_name().is_some_and(|n| n == "vendor") {
                    continue;
                }
                for entry in std::fs::read_dir(&path).map_err(|e| e.to_string())? {
                    pending.push(entry.map_err(|e| e.to_string())?.path());
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("php"))
            {
                files.insert(path);
            }
        }
    }
    Ok(files.into_iter().collect())
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches("./")
        .replace('\\', "/")
}

pub fn glob(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
    if path == pattern || path.starts_with(&format!("{pattern}/")) {
        return true;
    }
    let (mut p, mut s, mut star, mut retry) = (0, 0, None, 0);
    let (pattern, path) = (pattern.as_bytes(), path.as_bytes());
    while s < path.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == path[s]) {
            p += 1;
            s += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = s;
        } else if let Some(position) = star {
            p = position + 1;
            retry += 1;
            s = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

pub fn parse_files(paths: &[PathBuf]) -> Result<(Vec<ParsedFile>, Vec<Diagnostic>), String> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(paths.len());
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let next = &next;
            handles.push(
                std::thread::Builder::new()
                    .stack_size(phera_parser::PARSER_STACK_BYTES)
                    .spawn_scoped(scope, move || parse_worker(paths, next))
                    .map_err(|e| e.to_string())?,
            );
        }
        let (mut files, mut diagnostics) = (Vec::new(), Vec::new());
        for handle in handles {
            let (parsed, errors) = handle.join().map_err(|_| "Parser worker panicked")??;
            files.extend(parsed);
            diagnostics.extend(errors);
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok((files, diagnostics))
    })
}

fn parse_worker(
    paths: &[PathBuf],
    next: &AtomicUsize,
) -> Result<(Vec<ParsedFile>, Vec<Diagnostic>), String> {
    let (mut files, mut diagnostics) = (Vec::new(), Vec::new());
    while let Some(path) = paths.get(next.fetch_add(1, Ordering::Relaxed)) {
        // PHP source is bytes: a string literal may hold any of them (Symfony's
        // cache ships such a file). The parser takes text for now, so bytes
        // that are not UTF-8 become U+FFFD. That keeps every line and every
        // declaration where it is, which is all the checks read.
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes).into_owned();
        let lines = LineTable::new(&source);
        match phera_parser::parse_on_current_stack(&source) {
            Ok(program) => files.push(ParsedFile {
                path: display_path(path),
                source,
                lines,
                program,
            }),
            Err(error) => diagnostics.push(Diagnostic {
                file: display_path(path),
                line: lines.line(error.span.start),
                identifier: "phpstan.parse".into(),
                message: error.message,
            }),
        }
    }
    Ok((files, diagnostics))
}
