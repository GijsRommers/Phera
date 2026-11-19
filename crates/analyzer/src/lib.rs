//! Phera's static checker: `new Missing;` is diagnosed without running the file.
//! Parsing, declaration collection, flow analysis and rules share the runtime AST.

mod analysis;
pub mod baseline;
pub mod builtins;
pub mod collect;
mod collect_class;
mod composer;
pub mod config;
mod constant_types;
mod constants;
mod dependencies;
pub mod diagnostic;
mod enums;
pub mod global_effects;
pub mod index;
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
    let mut index = builtins::load_supported();
    index.read_type_docs = options.level >= 2;
    for file in &files {
        collect::collect(&mut index, &file.path, &file.source, &file.program);
    }
    for class in index.classes.values() {
        index.trait_users.record(class);
    }
    let scan = project::discover(&options.scan, &options.exclude)?;
    let mut dependencies = dependencies::Dependencies::load(&mut index, &options.paths, &paths)?;
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
    for file in &files {
        let mut found =
            analysis::Analyzer::new(&mut index, &mut dependencies, file, options.level).run();
        suppress::filter(file, &mut found);
        diagnostics.extend(found);
    }
    diagnostics.sort();
    diagnostics.dedup();
    Ok(Report {
        diagnostics,
        files: paths.len(),
        vendor_files: dependencies.vendor_files,
        elapsed: start.elapsed(),
    })
}
