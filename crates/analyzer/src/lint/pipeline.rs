//! Process PHP sources and commit a validated fix such as removing `use Unused;`.

use super::FixMode as Mode;
use super::{Enabled, Finding, MAX_FIX_PASSES, Options, analyze, fix, index};
use crate::{Report, baseline, diagnostic::Diagnostic, project};
use std::{collections::HashMap, path::Path, time::Instant};

pub fn run(options: &Options) -> Result<Report, String> {
    let start = Instant::now();
    let mut paths = project::discover(&options.paths, &options.exclude)?;
    if options.fix != Mode::None {
        let mut seen = std::collections::HashSet::new();
        paths.retain(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.clone())));
    }
    let enabled = Enabled::new(options);
    let (index, vendor_files) = if [
        "missing-parent-constructor",
        "unreachable-catch",
        "override-attribute",
        "str-contains",
        "str-starts-with",
    ]
    .iter()
    .any(|name| enabled.severities.contains_key(name))
    {
        let (index, vendor_files) = index::load(options, &paths)?;
        (Some(index), vendor_files)
    } else {
        (None, 0)
    };
    let files = project::parallel_files(
        &paths,
        options.workers.min(project::default_workers()),
        |path| lint_file(path, options, &enabled, index.as_ref()),
    )?;
    let mut diagnostics: Vec<_> = files.into_iter().flatten().collect();
    diagnostics.sort();
    if let Some(path) = &options.generate_baseline {
        baseline::write(path, &diagnostics)?;
    } else if let Some(path) = &options.baseline {
        baseline::filter(path, &mut diagnostics)?;
    }
    Ok(Report {
        diagnostics,
        files: paths.len(),
        vendor_files,
        elapsed: start.elapsed(),
    })
}

fn lint_file(
    path: &Path,
    options: &Options,
    enabled: &Enabled,
    index: Option<&crate::index::Index>,
) -> Result<Vec<Diagnostic>, String> {
    let original = project::read_source(path)?;
    let file = project::display_path(path);
    let (fixed, findings) = converge(&file, &original, options, enabled, index);
    if let Some(fixed) = fixed {
        fix::write(path, &original, &fixed).map_err(|error| format!("{file}: {error}"))?;
    }
    Ok(findings
        .into_iter()
        .map(|finding| finding.diagnostic)
        .collect())
}

pub(super) fn converge(
    file: &str,
    original: &[u8],
    options: &Options,
    enabled: &Enabled,
    index: Option<&crate::index::Index>,
) -> (Option<Vec<u8>>, Vec<Finding>) {
    let (_, mut findings) = analyze(file, original, options, enabled, index);
    let mut previous: Option<Vec<u8>> = None;
    if options.fix == Mode::None {
        return (previous, findings);
    }
    for pass in 0..=MAX_FIX_PASSES {
        let current = previous.as_deref().unwrap_or(original);
        let Some(next) = fix::apply(
            current,
            options.fix,
            findings.iter().filter_map(|finding| finding.fix.as_ref()),
        ) else {
            break;
        };
        let (valid, next_findings) = analyze(file, &next, options, enabled, index);
        if !valid || introduces_findings(&findings, &next_findings) {
            break;
        }
        if pass == MAX_FIX_PASSES {
            // Keep the original when rules require more than the bounded passes.
            return (None, analyze(file, original, options, enabled, index).1);
        }
        findings = next_findings;
        previous = Some(next);
    }
    (previous, findings)
}

fn introduces_findings(before: &[Finding], after: &[Finding]) -> bool {
    fn signature(finding: &Finding) -> (&str, &str) {
        (&finding.diagnostic.identifier, &finding.diagnostic.message)
    }
    let mut counts = HashMap::new();
    for finding in before {
        *counts.entry(signature(finding)).or_insert(0usize) += 1;
    }
    after.iter().any(|finding| {
        let Some(count) = counts.get_mut(&signature(finding)) else {
            return true;
        };
        if *count == 0 {
            return true;
        }
        *count -= 1;
        false
    })
}
