//! Phera's static checker: `new Missing;` is diagnosed without running the file.
//! Parsing, declaration collection, flow analysis and rules share the runtime AST.

mod analysis;
pub mod baseline;
pub mod builtins;
mod certainty;
pub mod collect;
mod collect_class;
mod composer;
pub mod config;
mod constant_types;
mod constants;
mod dependencies;
pub mod diagnostic;
mod enums;
mod framework_knowledge;
pub mod global_effects;
pub mod index;
#[cfg(test)]
mod ladder_equivalence;
pub mod lint;
pub mod names;
mod phpdoc;
pub mod project;
pub mod rules;
pub mod scope;
mod signature_types;
pub mod stubs;
mod suppress;
mod trait_users;
mod traits;
pub mod types;
mod walk;

use config::Options;
use diagnostic::Diagnostic;

pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
    pub files: usize,
    pub vendor_files: usize,
    pub elapsed: std::time::Duration,
}

pub fn check(options: &Options) -> Result<Report, String> {
    // Analysis, AST destruction and bounded nested walks share the parser's stack budget.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("phera-check".into())
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, || check_project(options))
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "Analyzer worker panicked".to_string())?
    })
}

fn check_project(options: &Options) -> Result<Report, String> {
    let start = std::time::Instant::now();
    let paths = project::discover(&options.paths, &options.exclude)?;
    let (files, mut diagnostics) = project::parse_files(&paths)?;
    let (found, vendor_files) = analyze_files(&files, &paths, options)?;
    diagnostics.extend(found);
    diagnostics.sort();
    diagnostics.dedup();
    if options.generate_baseline.is_none() {
        config::Ignore::filter(&options.phpstan_ignores, &mut diagnostics);
    }
    Ok(Report {
        diagnostics,
        files: paths.len(),
        vendor_files,
        elapsed: start.elapsed(),
    })
}

/// Collects the declarations of parsed files, resolves their dependencies and
/// analyses each one; returns the findings and the number of vendor files read.
pub(crate) fn analyze_files(
    files: &[project::ParsedFile],
    paths: &[std::path::PathBuf],
    options: &Options,
) -> Result<(Vec<Diagnostic>, usize), String> {
    let (mut findings, mut vendor_files) = analyze_types(files, paths, options, true)?;
    if !options.treat_phpdoc_types_as_certain && findings.iter().any(certainty::required) {
        let (native, native_vendor_files) = analyze_types(files, paths, options, false)?;
        certainty::retain_native(&mut findings, &native);
        vendor_files = vendor_files.max(native_vendor_files);
    }
    Ok((findings, vendor_files))
}

fn analyze_types(
    files: &[project::ParsedFile],
    paths: &[std::path::PathBuf],
    options: &Options,
    read_docs: bool,
) -> Result<(Vec<Diagnostic>, usize), String> {
    let mut diagnostics = Vec::new();
    let mut index = builtins::load_supported();
    index.report_maybe_generic_types = options.level >= 7;
    index.read_type_docs = read_docs
        && (options.level >= 2
            || options.strict
            || options.check_missing_typehints == Some(true)
            || options.check_phpdoc_missing_return == Some(true)
            || options.level >= 1 && options.unnecessary_null_coalesce);
    if options.level >= 4 {
        builtins::regex_version::add_constants(&mut index);
    }
    if options.level >= 7 {
        builtins::analysis_facts::refine(&mut index);
    }
    for file in files {
        collect::collect(&mut index, &file.path, &file.source, &file.program);
    }
    for class in index.classes.values() {
        index.trait_users.record(class);
    }
    let scan = project::discover(&options.scan, &options.exclude)?;
    let mut dependencies = dependencies::Dependencies::load(&mut index, &options.paths, paths)?;
    for path in &scan {
        dependencies.load_file(&mut index, path)?;
    }
    let classes = index
        .classes
        .values()
        .map(|class| class.name.clone())
        .collect();
    dependencies.resolve_classes(&mut index, classes);
    traits::link(&mut index);
    let report_unused_traits = options.level >= 4
        && options
            .paths
            .iter()
            .any(|path| std::path::Path::new(path).is_dir());
    for file in files {
        let found = analysis::Analyzer::new(
            &mut index,
            &mut dependencies,
            file,
            options,
            report_unused_traits,
        )
        .run();
        diagnostics.extend(found);
    }
    Ok((diagnostics, dependencies.vendor_files))
}
