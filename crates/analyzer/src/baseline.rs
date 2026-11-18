//! Exact diagnostic baselines: fixing or moving `missing()` invalidates its old entry.
//! Version 1 stores the same objects as JSON output, one object per line.

use crate::diagnostic::Diagnostic;
use std::collections::BTreeSet;

pub fn write(path: &str, diagnostics: &[Diagnostic]) -> Result<(), String> {
    let entries = diagnostics
        .iter()
        .map(Diagnostic::json)
        .collect::<Vec<_>>()
        .join(",\n");
    let content = format!("{{\"version\":1,\"diagnostics\":[\n{entries}\n]}}\n");
    std::fs::write(path, content).map_err(|e| format!("{path}: {e}"))
}

pub fn filter(path: &str, diagnostics: &mut Vec<Diagnostic>) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    if !content.starts_with("{\"version\":1,\"diagnostics\":[\n") || !content.ends_with("]}\n") {
        return Err(format!(
            "{path}: expected a version 1 baseline from --generate-baseline"
        ));
    }
    let entries: BTreeSet<_> = content
        .lines()
        .skip(1)
        .filter(|l| l.starts_with('{'))
        .map(|l| l.trim_end_matches(','))
        .collect();
    diagnostics.retain(|d| !entries.contains(d.json().as_str()));
    Ok(())
}
