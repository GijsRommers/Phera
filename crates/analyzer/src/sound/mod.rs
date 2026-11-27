//! Closed-world analysis for `phera build`: which files make up a program,
//! what they declare, and where the program can still leave what was built.
//!
//! ```text
//! discover_initial      entries, Composer's tables, framework files, [build].include
//! close_and_analyze     compile, follow `require __DIR__ . '/x.php'`, then:
//!                         declarations   every function and class, counted
//!                         hierarchy      who extends whom
//!                         audit          eval, include $file, new $class, ...
//!                         function facts which variables may be references
//! ```
//!
//! Everything after discovery reads compiled bytecode only. A fact is
//! recorded when PHP's run-time rules make it true; a doc comment is never
//! evidence. Where the analysis cannot tell, it records the cautious answer:
//! a flag that warns is set, a flag that proves is left out.
//!
//! This is separate from `phera check`: that reports what looks wrong to a
//! person, this records what a compiler may rely on.

mod audit;
mod composer;
mod control;
mod declarations;
mod discover;
mod framework;
mod function_facts;
mod glob;
mod hierarchy;
mod includes;
mod report;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use phera_bytecode::facts::{
    Assumptions, BuildMode, ClassRef, DynamicAudit, DynamicFinding, DynamicKind, ProgramFacts,
    UnitBuildFacts, UnitRef,
};
use phera_bytecode::{ClassId, ClassKind, CompiledUnit};

/// What to build and how strictly.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub project_root: PathBuf,
    pub mode: BuildMode,
    /// Entry scripts, relative to `project_root`. Empty: `public/index.php`
    /// or `index.php`, and a detected framework's console script.
    pub entries: Vec<PathBuf>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub include_dev: bool,
    pub allow: Vec<DynamicKind>,
    /// Lowercased native function names provided by the runtime `Registry`.
    pub native_functions: BTreeSet<String>,
    /// Lowercased native class/interface/trait names provided by the runtime `Registry`.
    pub native_classes: BTreeSet<String>,
    /// Lowercased parent/interface edges for native and runtime-shipped bundled
    /// classes (e.g. `phera\symfony\runtime -> [symfony\component\runtime\symfonyruntime]`).
    pub native_supertypes: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// Canonical path.
    pub path: PathBuf,
    /// Relative to canonical `project_root` with `/` separators when inside root, else canonical string.
    pub relative_path: String,
    /// A Composer `autoload.files` entry: required on every request.
    pub always_loaded: bool,
    pub is_entry: bool,
}

#[derive(Debug, Clone)]
pub struct DiscoveredWorld {
    /// Canonical.
    pub project_root: PathBuf,
    /// Sorted by `relative_path`.
    pub files: Vec<DiscoveredFile>,
    /// Composer `autoload.files` in exact load order (canonical paths).
    pub autoload_files: Vec<PathBuf>,
    /// Lowercased class name -> canonical path from Composer classmap + PSR-4 scan.
    pub classmap: BTreeMap<String, PathBuf>,
    /// Detected framework if any (`Some("laravel")` or `Some("symfony")`).
    pub framework: Option<&'static str>,
    /// Files and directories that Composer's tables or `[build].include`
    /// name and that do not exist.
    pub skipped_missing: usize,
}

#[derive(Debug, Clone)]
pub struct CompiledWorldUnit {
    pub file: DiscoveredFile,
    pub source_len: u64,
    pub source_hash: [u8; 16],
    pub unit: Arc<CompiledUnit>,
}

/// A discovered file that could not be read or compiled and was left out.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompileFailure {
    pub relative_path: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct AnalyzedWorld {
    /// The discovered world after closing: `files` lists exactly the
    /// compiled units, in the same order as `units`.
    pub world: DiscoveredWorld,
    /// Sorted by relative path. The index of a unit is its [`UnitRef`].
    pub units: Vec<CompiledWorldUnit>,
    pub program_facts: ProgramFacts,
    /// Aligned 1:1 with `units`.
    pub unit_facts: Vec<UnitBuildFacts>,
    /// Sorted classmap entries: `(lowercased_class_name, file_index, class_id, class_ref)`.
    ///
    /// One entry per named class declaration, so a name declared in two
    /// files has two neighbouring entries; `program_facts.classes[class_ref]`
    /// says `Duplicate` for it, and `world.classmap` which file Composer
    /// would load. Anonymous classes are not listed.
    pub classmap: Vec<(String, u32, ClassId, ClassRef)>,
    /// Files left out because they could not be read or compiled, sorted.
    /// PHP fails on such a file only when it is loaded; so does the image.
    pub compile_failures: Vec<CompileFailure>,
}

/// Finds the files of a build without compiling any: the entry scripts,
/// what Composer can autoload, what a detected framework includes by a
/// computed path, and what `include_globs` name.
pub fn discover_initial(config: &BuildConfig) -> Result<DiscoveredWorld, String> {
    discover::discover(config)
}

/// Compiles the world, adds every file a static `include` names until none
/// is new, and analyzes the result.
///
/// `compile` gets a file's canonical path and its bytes. A file that fails
/// to compile is left out and listed in [`AnalyzedWorld::compile_failures`];
/// an entry script or a Composer `autoload.files` entry that fails is an
/// error.
pub fn close_and_analyze<C, H>(
    config: &BuildConfig,
    world: DiscoveredWorld,
    compile: C,
    hash_source: H,
) -> Result<AnalyzedWorld, String>
where
    C: Fn(&Path, &[u8]) -> Result<CompiledUnit, String>,
    H: Fn(&[u8]) -> [u8; 16],
{
    let exclude = glob::GlobSet::new(&config.exclude_globs);
    let closed = includes::close(world, &exclude, &compile, &hash_source)?;
    let units = closed.units;
    if u32::try_from(units.len()).is_err() {
        return Err(format!(
            "{} files are more than a build can hold",
            units.len()
        ));
    }

    let declarations = declarations::Declarations::collect(
        &units,
        &config.native_functions,
        &config.native_classes,
    );
    let mut hierarchy =
        hierarchy::Hierarchy::build(&units, &declarations, &config.native_supertypes);
    let audit = audit::audit(
        &units,
        &closed.static_includes,
        &hierarchy,
        config.mode,
        &config.allow,
    );
    hierarchy.mark_effectively_final(audit.holds.contains(Assumptions::SEALED_HIERARCHY));

    let callees = function_facts::Callees::new(
        &units,
        &declarations,
        audit.holds.contains(Assumptions::SEALED_NAMES),
    );
    let unit_facts: Vec<UnitBuildFacts> = units
        .iter()
        .zip(&hierarchy.unit_classes)
        .enumerate()
        .map(|(index, (unit, classes))| {
            function_facts::unit_facts(UnitRef(index as u32), &unit.unit, classes.clone(), &callees)
        })
        .collect();

    let mut classmap = Vec::new();
    for (index, (unit, classes)) in units.iter().zip(&hierarchy.unit_classes).enumerate() {
        for (class, declaration) in unit.unit.classes.iter().enumerate() {
            if declaration.kind != ClassKind::Anonymous {
                classmap.push((
                    declaration.name.to_ascii_lowercase(),
                    index as u32,
                    ClassId(class as u32),
                    classes[class],
                ));
            }
        }
    }
    classmap.sort();

    let program_facts = ProgramFacts {
        mode: config.mode,
        units: units
            .iter()
            .map(|unit| unit.file.relative_path.as_str().into())
            .collect(),
        functions: declarations.function_table(),
        classes: hierarchy.classes,
        descendants: hierarchy.descendants,
        audit,
    };
    Ok(AnalyzedWorld {
        world: closed.world,
        units,
        program_facts,
        unit_facts,
        classmap,
        compile_failures: closed.compile_failures,
    })
}

/// Returns findings that violate `BuildMode::Strict` given the `allow` list.
/// (In Hybrid mode, always returns empty.)
pub fn strict_violations<'a>(
    mode: BuildMode,
    audit: &'a DynamicAudit,
    allow: &[DynamicKind],
) -> Vec<&'a DynamicFinding> {
    audit::strict_violations(mode, audit, allow)
}

/// The audit as lines for a terminal.
pub fn audit_report_text(analyzed: &AnalyzedWorld) -> String {
    report::text(analyzed)
}

/// The audit as a JSON document with the same content as the text report.
pub fn audit_report_json(analyzed: &AnalyzedWorld) -> String {
    report::json(analyzed)
}

/// A path as a build names it: relative to the project root with `/`
/// separators, or the whole path for a file outside the project.
pub(crate) fn relative_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
