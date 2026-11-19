//! Reproducible builtin generation from PHP's own stubs, e.g. `DateTime::format()`.
//! A committed table keeps php-src and PHP out of Phera's normal build dependency graph.

mod extensions;
mod normalize;
mod source;
mod supplement;

use crate::{builtins, collect, index::Index};
use std::path::Path;

pub use extensions::generate as generate_extensions;

pub fn generate(root: &Path, reference: &str) -> Result<String, String> {
    let source = source::StubSource::open(root, reference)?;
    let mut report = format!(
        "PHP {} stubs; php-src: {}\nReference: {reference}\n",
        source.level.name(),
        source.provenance
    );
    let (mut index, mut parsed, mut skipped, mut aliases) = (Index::default(), 0, 0, 0);
    for file in source.files()? {
        let original = source.read(&file)?;
        let normalized = normalize::normalize(&file, &original, &mut report)?;
        match phera_parser::parse(&normalized) {
            Ok(program) => {
                collect::collect(&mut index, &file, &normalized, &program);
                aliases += supplement::aliases(&mut index, &file, &normalized);
                parsed += 1;
            }
            Err(error) => {
                let line = phera_source::LineTable::new(&normalized).line(error.span.start);
                report.push_str(&format!("SKIPPED {file}:{line}: {}\n", error.message));
                skipped += 1;
            }
        }
    }
    let supplemental = supplement::add(&mut index, &source)?;
    let members: usize = index
        .classes
        .values()
        .map(|c| c.methods.len() + c.properties.len() + c.constants.len())
        .sum();
    let declarations =
        index.functions.len() + index.constants.len() + index.classes.len() + members;
    report.push_str(&format!(
        "Parsed files: {parsed}; skipped files: {skipped}\n"
    ));
    report.push_str(&format!(
        "Declarations: {declarations}; functions: {}; classes: {}; globals: {}; \
        members: {members}\n",
        index.functions.len(),
        index.classes.len(),
        index.constants.len()
    ));
    report.push_str(&format!(
        "Supplemental constants: {supplemental}; class aliases: {aliases}\n"
    ));
    report.push_str("Excluded development-only ext/zend_test and ext/dl_test.\n");
    // A parse failure cannot be silently committed as an incomplete PHP builtin table.
    if skipped != 0 {
        return Err(report);
    }
    report.push_str("Skipped declarations: 0. Parser changes: none.\n");
    let data = format!(
        "# php-src {}\n{}",
        source.provenance,
        builtins::encode(&index)
    );
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    let (table, log) = match source.level {
        phera_source::LanguageLevel::Php85 => ("php85.tsv", "generation.txt"),
        phera_source::LanguageLevel::Php86 => ("php86.tsv", "generation-php86.txt"),
    };
    std::fs::write(destination.join(table), data).map_err(|e| e.to_string())?;
    std::fs::write(destination.join(log), &report).map_err(|e| e.to_string())?;
    Ok(report)
}
