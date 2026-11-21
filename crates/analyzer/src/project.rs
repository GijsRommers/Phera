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
    let parsed = parallel_files(paths, default_workers(), parse_file)?;
    let (mut files, mut diagnostics) = (Vec::new(), Vec::new());
    for result in parsed {
        match result {
            Ok(file) => files.push(file),
            Err(error) => diagnostics.push(error),
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, diagnostics))
}

/// Default to one worker per core when checking or linting a directory.
pub(crate) fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Each worker owns the stack budget required to parse and drop PHP syntax trees.
pub(crate) fn parallel_files<T: Send>(
    paths: &[PathBuf],
    workers: usize,
    visit: impl Fn(&Path) -> Result<T, String> + Sync,
) -> Result<Vec<T>, String> {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers.max(1).min(paths.len()) {
            let (next, visit) = (&next, &visit);
            handles.push(
                std::thread::Builder::new()
                    .stack_size(phera_parser::PARSER_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        let mut results = Vec::new();
                        while let Some(path) = paths.get(next.fetch_add(1, Ordering::Relaxed)) {
                            results.push(visit(path)?);
                        }
                        Ok::<_, String>(results)
                    })
                    .map_err(|e| e.to_string())?,
            );
        }
        let mut results = Vec::new();
        for handle in handles {
            results.extend(handle.join().map_err(|_| "Analyzer worker panicked")??);
        }
        Ok(results)
    })
}

pub(crate) fn read_source(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn parse_file(path: &Path) -> Result<Result<ParsedFile, Diagnostic>, String> {
    // PHP source is bytes. Keep every line in files with non-UTF-8 literals,
    // even though the parser currently takes text.
    let bytes = read_source(path)?;
    let source = String::from_utf8_lossy(&bytes).into_owned();
    let lines = LineTable::new(&source);
    let path = display_path(path);
    let (parsed, _) = phera_parser::parse_file_on_current_stack(source.as_bytes(), true);
    Ok(match parsed {
        Ok(program) => Ok(ParsedFile {
            path,
            source,
            lines,
            program,
        }),
        Err(error) => Err(Diagnostic {
            severity: crate::diagnostic::Severity::Error,
            file: path,
            line: lines.line(error.span.start),
            identifier: "phpstan.parse".into(),
            message: error.message,
        }),
    })
}
