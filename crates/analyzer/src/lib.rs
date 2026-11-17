//! Phera's static checker: `new Missing;` is diagnosed without running the file.
//! Parsing, declaration collection, flow analysis and rules share the runtime AST.

mod analysis;
pub mod baseline;
pub mod builtins;
pub mod collect;
mod collect_class;
pub mod config;
mod constants;
pub mod diagnostic;
mod enums;
pub mod index;
pub mod names;
pub mod project;
pub mod rules;
pub mod scope;
pub mod stubs;
mod suppress;
mod traits;
pub mod types;
mod walk;

use config::Options;
use diagnostic::Diagnostic;

pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
    pub files: usize,
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
    let mut index = builtins::load();
    for file in &files {
        collect::collect(&mut index, &file.path, &file.source, &file.program);
    }
    traits::link(&mut index);
    for file in &files {
        let found = analysis::Analyzer::new(&index, file, options.level).run();
        diagnostics.extend(found.into_iter().filter(|d| !suppress::suppressed(file, d)));
    }
    diagnostics.sort();
    diagnostics.dedup();
    Ok(Report {
        diagnostics,
        files: paths.len(),
        elapsed: start.elapsed(),
    })
}
