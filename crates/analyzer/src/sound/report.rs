//! The audit for people and for tools:
//!
//! ```text
//! Closed-world audit: strict build of 412 files (laravel)
//!   classes 1873, functions 96
//!   holds: sealed names, sealed hierarchy
//!
//! Dynamic features: 2
//!   eval                 1  names
//!   dynamic_new          1  class
//!
//! Findings
//!   app/Console/Tinker.php:42  eval [names]  App\Console\Tinker::run: eval()
//! ```

use std::collections::BTreeMap;
use std::fmt::Write;

use phera_bytecode::facts::{Assumptions, BlastRadius, BuildMode, DynamicFinding, DynamicKind};
use serde_json::{Value, json};

use super::AnalyzedWorld;

pub(crate) fn text(analyzed: &AnalyzedWorld) -> String {
    let facts = &analyzed.program_facts;
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "Closed-world audit: {} build of {} files{}",
        mode_name(facts.mode),
        analyzed.units.len(),
        analyzed
            .world
            .framework
            .map(|framework| format!(" ({framework})"))
            .unwrap_or_default()
    );
    let _ = writeln!(
        out,
        "  classes {}, functions {}",
        facts.classes.len(),
        facts.functions.len()
    );
    let _ = writeln!(out, "  holds: {}", holds_text(facts.audit.holds));
    if analyzed.world.skipped_missing > 0 {
        let _ = writeln!(
            out,
            "  named but missing on disk: {}",
            analyzed.world.skipped_missing
        );
    }

    let _ = writeln!(out, "\nDynamic features: {}", facts.audit.findings.len());
    for ((kind, radius), count) in summary(&facts.audit.findings) {
        let _ = writeln!(
            out,
            "  {:<20} {count:>5}  {}",
            kind.as_str(),
            radius.as_str()
        );
    }
    if !facts.audit.findings.is_empty() {
        let _ = writeln!(out, "\nFindings");
        for finding in facts.audit.findings.iter() {
            let (file, function) = place(analyzed, finding);
            let _ = writeln!(
                out,
                "  {file}:{}  {} [{}]  {function}: {}",
                finding.line,
                finding.kind.as_str(),
                finding.radius.as_str(),
                finding.detail
            );
        }
    }
    if !analyzed.compile_failures.is_empty() {
        let _ = writeln!(out, "\nNot compiled: {}", analyzed.compile_failures.len());
        for failure in &analyzed.compile_failures {
            let _ = writeln!(out, "  {}: {}", failure.relative_path, failure.message);
        }
    }
    out
}

pub(crate) fn json(analyzed: &AnalyzedWorld) -> String {
    let facts = &analyzed.program_facts;
    let findings: Vec<Value> = facts
        .audit
        .findings
        .iter()
        .map(|finding| {
            let (file, function) = place(analyzed, finding);
            json!({
                "kind": finding.kind.as_str(),
                "radius": finding.radius.as_str(),
                "file": file,
                "function": function,
                "line": finding.line,
                "detail": &*finding.detail,
            })
        })
        .collect();
    let summary: BTreeMap<&str, usize> = summary(&facts.audit.findings)
        .into_iter()
        .map(|((kind, _), count)| (kind.as_str(), count))
        .collect();
    let failures: Vec<Value> = analyzed
        .compile_failures
        .iter()
        .map(|failure| json!({"file": failure.relative_path, "message": failure.message}))
        .collect();
    let document = json!({
        "mode": mode_name(facts.mode),
        "framework": analyzed.world.framework,
        "files": analyzed.units.len(),
        "classes": facts.classes.len(),
        "functions": facts.functions.len(),
        "skipped_missing": analyzed.world.skipped_missing,
        "holds": {
            "sealed_names": facts.audit.holds.contains(Assumptions::SEALED_NAMES),
            "sealed_hierarchy": facts.audit.holds.contains(Assumptions::SEALED_HIERARCHY),
        },
        "summary": summary,
        "findings": findings,
        "compile_failures": failures,
    });
    // A `Value` always serializes; keys come out sorted.
    let mut text = serde_json::to_string_pretty(&document).unwrap_or_default();
    text.push('\n');
    text
}

fn mode_name(mode: BuildMode) -> &'static str {
    match mode {
        BuildMode::Hybrid => "hybrid",
        BuildMode::Strict => "strict",
    }
}

fn holds_text(holds: Assumptions) -> String {
    let mut names = Vec::new();
    if holds.contains(Assumptions::SEALED_NAMES) {
        names.push("sealed names");
    }
    if holds.contains(Assumptions::SEALED_HIERARCHY) {
        names.push("sealed hierarchy");
    }
    if names.is_empty() {
        "nothing (code outside the build can be loaded at run time)".to_owned()
    } else {
        names.join(", ")
    }
}

/// How often each kind occurs, in the order the kinds are declared.
fn summary(findings: &[DynamicFinding]) -> Vec<((DynamicKind, BlastRadius), usize)> {
    let mut counts: Vec<((DynamicKind, BlastRadius), usize)> = Vec::new();
    for finding in findings {
        let key = (finding.kind, finding.radius);
        match counts.iter_mut().find(|(counted, _)| *counted == key) {
            Some((_, count)) => *count += 1,
            None => counts.push((key, 1)),
        }
    }
    counts.sort_by_key(|((kind, radius), _)| (*kind as u8, *radius as u8));
    counts
}

/// The file and the function a finding is in, as a person reads them.
fn place(analyzed: &AnalyzedWorld, finding: &DynamicFinding) -> (String, String) {
    let Some(unit) = analyzed.units.get(finding.at.unit.0 as usize) else {
        return ("?".to_owned(), "?".to_owned());
    };
    let function = match finding.at.function.checked_sub(1) {
        None => Some(&unit.unit.main),
        Some(index) => unit.unit.functions.get(index as usize),
    };
    // An anonymous class is named `class@anonymous`, a zero byte, then its
    // file and line; the part before the zero byte is what PHP prints.
    let name = function
        .map(|function| function.name.split('\0').next().unwrap_or_default())
        .unwrap_or("?");
    (unit.file.relative_path.clone(), name.to_owned())
}
