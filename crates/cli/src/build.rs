//! `phera build`: closed-world discovery, sound whole-program audit, and `.phb`
//! production image compilation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use phera_analyzer::sound::{
    self, AnalyzedWorld, BuildConfig, audit_report_json, audit_report_text, close_and_analyze,
    discover_initial, strict_violations,
};
use phera_aot::{
    AotImage, FileFlags, ImageClassEntry, ImageFileEntry, ImageInput, ImageManifest, ImageTarget,
    blake2b_128, encode_image, write_encoded_image,
};
use phera_bytecode::facts::{BuildMode, DynamicKind};

pub(crate) const HELP: &str = "\
Usage:
  phera build [OPTIONS] [PATH]            compile a project into a .phb AOT image

Options:
  -o, --output FILE                       output image path (default: var/phera/app.phb)
  --mode hybrid|strict                    build mode (default: hybrid)
  --entry PATH                            entrypoint PHP script (repeatable)
  --include GLOB                          extra PHP files/globs to include (repeatable)
  --exclude GLOB                          paths/globs to exclude (repeatable)
  --target web|cli|both                   runtime target(s) to compile (default: web)
  --no-dev                                exclude Composer autoload-dev and dev packages
  --allow KIND                            allow a dynamic feature in strict mode (repeatable:
                                          eval, dynamic_include, dynamic_new, dynamic_static_call,
                                          dynamic_method_call, class_alias, extract, compact,
                                          unserialize, reflection, variable_variable)
  --audit[=text|json]                     print the closed-world dynamic-feature audit report
  --audit-only                            run discovery & audit without writing an image
  --verify                                reopen and validate every unit in the written image
  -h, --help                              show this help";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuildTarget {
    Web,
    Cli,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuditFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub(crate) struct BuildOptions {
    pub path: PathBuf,
    pub output: Option<PathBuf>,
    pub mode: Option<BuildMode>,
    pub entries: Vec<PathBuf>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub target: BuildTarget,
    pub no_dev: bool,
    pub allow: Vec<DynamicKind>,
    pub audit: Option<AuditFormat>,
    pub audit_only: bool,
    pub verify: bool,
}

impl BuildOptions {
    pub(crate) fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            path: PathBuf::from("."),
            output: None,
            mode: None,
            entries: Vec::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            target: BuildTarget::Web,
            no_dev: false,
            allow: Vec::new(),
            audit: None,
            audit_only: false,
            verify: false,
        };
        let mut path_given = false;
        while let Some(arg) = arguments.next() {
            match arg.as_str() {
                "-o" | "--output" => {
                    let value = arguments.next().ok_or("--output needs a file path.")?;
                    options.output = Some(PathBuf::from(value));
                }
                "--mode" => {
                    let value = arguments
                        .next()
                        .ok_or("--mode needs `hybrid` or `strict`.")?;
                    options.mode = Some(parse_mode(&value)?);
                }
                "--entry" => {
                    let value = arguments.next().ok_or("--entry needs a PHP path.")?;
                    options.entries.push(PathBuf::from(value));
                }
                "--include" => {
                    let value = arguments.next().ok_or("--include needs a glob pattern.")?;
                    options.include.push(value);
                }
                "--exclude" => {
                    let value = arguments.next().ok_or("--exclude needs a glob pattern.")?;
                    options.exclude.push(value);
                }
                "--target" => {
                    let value = arguments
                        .next()
                        .ok_or("--target needs `web`, `cli`, or `both`.")?;
                    options.target = match value.as_str() {
                        "web" => BuildTarget::Web,
                        "cli" => BuildTarget::Cli,
                        "both" => BuildTarget::Both,
                        _ => {
                            return Err(format!(
                                "Unknown --target `{value}`. Expected `web`, `cli`, or `both`."
                            ));
                        }
                    };
                }
                "--no-dev" => options.no_dev = true,
                "--allow" => {
                    let value = arguments.next().ok_or("--allow needs a feature name.")?;
                    for part in value.split(',') {
                        let trimmed = part.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let kind = DynamicKind::parse_allow(trimmed).ok_or_else(|| {
                            format!("Unknown --allow dynamic feature `{trimmed}`.")
                        })?;
                        if !options.allow.contains(&kind) {
                            options.allow.push(kind);
                        }
                    }
                }
                "--audit" => options.audit = Some(AuditFormat::Text),
                "--audit=text" => options.audit = Some(AuditFormat::Text),
                "--audit=json" => options.audit = Some(AuditFormat::Json),
                "--audit-only" => {
                    options.audit_only = true;
                    if options.audit.is_none() {
                        options.audit = Some(AuditFormat::Text);
                    }
                }
                "--verify" => options.verify = true,
                other if other.starts_with("--audit=") => {
                    return Err(format!(
                        "Unknown --audit format `{}`. Expected `text` or `json`.",
                        &other["--audit=".len()..]
                    ));
                }
                other if other.starts_with('-') => {
                    return Err(format!("Unknown build option: {other}"));
                }
                other => {
                    if path_given {
                        return Err("phera build takes at most one project path.".into());
                    }
                    path_given = true;
                    options.path = PathBuf::from(other);
                }
            }
        }
        Ok(options)
    }
}

pub(crate) fn run(options: BuildOptions) -> ExitCode {
    match execute(options) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("phera build: {message}");
            ExitCode::FAILURE
        }
    }
}

fn execute(options: BuildOptions) -> Result<ExitCode, String> {
    let started = Instant::now();
    let project_root = options
        .path
        .canonicalize()
        .map_err(|error| format!("cannot open `{}`: {error}", options.path.display()))?;
    let toml_cfg = read_build_toml(&project_root)?;

    let mode = options.mode.or(toml_cfg.mode).unwrap_or(BuildMode::Hybrid);
    let output_path = options
        .output
        .or(toml_cfg.output)
        .unwrap_or_else(|| PathBuf::from("var/phera/app.phb"));
    let output_path = if output_path.is_absolute() {
        output_path
    } else {
        project_root.join(output_path)
    };

    let mut entries = if options.entries.is_empty() {
        toml_cfg.entries
    } else {
        options.entries
    };
    entries.sort();
    entries.dedup();

    let mut include_globs = toml_cfg.include;
    include_globs.extend(options.include);
    include_globs.sort();
    include_globs.dedup();

    let mut exclude_globs = toml_cfg.exclude;
    exclude_globs.extend(options.exclude);
    exclude_globs.sort();
    exclude_globs.dedup();

    let mut allow = toml_cfg.allow;
    for kind in options.allow {
        if !allow.contains(&kind) {
            allow.push(kind);
        }
    }

    let primary_target = match options.target {
        BuildTarget::Web | BuildTarget::Both => ImageTarget::Web,
        BuildTarget::Cli => ImageTarget::Cli,
    };
    let inventory = phera_runtime::image::native_inventory(primary_target);

    let build_config = BuildConfig {
        project_root: project_root.clone(),
        mode,
        entries: entries.clone(),
        include_globs: include_globs.clone(),
        exclude_globs: exclude_globs.clone(),
        include_dev: !options.no_dev,
        allow: allow.clone(),
        native_functions: inventory.functions,
        native_classes: inventory.classes,
        native_supertypes: inventory.supertypes,
    };

    let initial = discover_initial(&build_config)?;
    let analyzed = close_and_analyze(
        &build_config,
        initial.clone(),
        |path, bytes| {
            let file_str = path.to_string_lossy();
            phera_runtime::image::compile_bytes_for_target(bytes, &file_str, primary_target)
                .map_err(|error| format!("{}: {}", path.display(), error.message))
        },
        blake2b_128,
    )?;

    let cli_analyzed = if options.target == BuildTarget::Both {
        let cli_inventory = phera_runtime::image::native_inventory(ImageTarget::Cli);
        let cli_config = BuildConfig {
            native_functions: cli_inventory.functions,
            native_classes: cli_inventory.classes,
            native_supertypes: cli_inventory.supertypes,
            ..build_config.clone()
        };
        Some(close_and_analyze(
            &cli_config,
            initial,
            |path, bytes| {
                let file_str = path.to_string_lossy();
                phera_runtime::image::compile_bytes_for_target(bytes, &file_str, ImageTarget::Cli)
                    .map_err(|error| format!("{}: {}", path.display(), error.message))
            },
            blake2b_128,
        )?)
    } else {
        None
    };

    if let Some(format) = options.audit {
        match format {
            AuditFormat::Text => print!("{}", audit_report_text(&analyzed)),
            AuditFormat::Json => println!("{}", audit_report_json(&analyzed)),
        }
    }

    let violations = strict_violations(mode, &analyzed.program_facts.audit, &allow);
    if !violations.is_empty() {
        eprintln!(
            "phera build: strict mode rejected {} dynamic feature finding(s):",
            violations.len()
        );
        for finding in &violations {
            let unit_path = analyzed
                .program_facts
                .units
                .get(finding.at.unit.0 as usize)
                .map(Box::as_ref)
                .unwrap_or("<unknown>");
            eprintln!(
                "  - [{}] {}:{}: {} (pass --allow {} to permit)",
                finding.kind.as_str(),
                unit_path,
                finding.line,
                finding.detail,
                finding.kind.as_str()
            );
        }
        return Ok(ExitCode::FAILURE);
    }
    if let Some(cli) = &cli_analyzed {
        let cli_violations = strict_violations(mode, &cli.program_facts.audit, &allow);
        if !cli_violations.is_empty() {
            eprintln!(
                "phera build: strict mode rejected {} CLI-target dynamic feature finding(s):",
                cli_violations.len()
            );
            for finding in &cli_violations {
                let unit_path = cli
                    .program_facts
                    .units
                    .get(finding.at.unit.0 as usize)
                    .map(Box::as_ref)
                    .unwrap_or("<unknown>");
                eprintln!(
                    "  - [{}] {}:{}: {} (pass --allow {} to permit)",
                    finding.kind.as_str(),
                    unit_path,
                    finding.line,
                    finding.detail,
                    finding.kind.as_str()
                );
            }
            return Ok(ExitCode::FAILURE);
        }
    }

    if options.audit_only {
        return Ok(ExitCode::SUCCESS);
    }

    let skipped_compile = analyzed.compile_failures.len();
    let input = assemble_image_input(
        &project_root,
        mode,
        options.target,
        &allow,
        &include_globs,
        &exclude_globs,
        !options.no_dev,
        analyzed,
        cli_analyzed,
    )?;
    let bytes = encode_image(&input).map_err(|error| error.to_string())?;

    // Verify in-memory BEFORE atomically replacing any existing image on disk.
    if options.verify {
        let reopened = AotImage::from_bytes(bytes.clone().into(), Some(input.runtime_abi_hash))
            .map_err(|error| format!("verification failed: {error}"))?;
        reopened
            .verify_project_root(&project_root)
            .map_err(|error| format!("verification failed: {error}"))?;
        let verified_units = reopened
            .verify_all_units()
            .map_err(|error| format!("verification failed: {error}"))?;
        eprintln!(
            "Verified {verified_units} units in {}",
            output_path.display()
        );
    }

    let bytes_written =
        write_encoded_image(&output_path, &bytes).map_err(|error| error.to_string())?;

    let skipped_note = if skipped_compile > 0 {
        format!(", {skipped_compile} skipped (compile errors)")
    } else {
        String::new()
    };
    eprintln!(
        "Built {} ({} files, {} units, {} classes, {} bytes{skipped_note}) in {} ms",
        output_path.display(),
        input.files.len(),
        input.units.len(),
        input.classmap.len(),
        bytes_written,
        started.elapsed().as_millis()
    );
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
fn assemble_image_input(
    project_root: &Path,
    mode: BuildMode,
    target: BuildTarget,
    allow: &[DynamicKind],
    include_globs: &[String],
    exclude_globs: &[String],
    include_dev: bool,
    analyzed: AnalyzedWorld,
    cli_analyzed: Option<AnalyzedWorld>,
) -> Result<ImageInput, String> {
    let has_web = matches!(target, BuildTarget::Web | BuildTarget::Both);
    let has_cli = matches!(target, BuildTarget::Cli | BuildTarget::Both);

    let (mut cli_by_rel, cli_class_names) = match cli_analyzed {
        Some(cli) => {
            let names: Vec<Box<str>> = cli
                .program_facts
                .classes
                .iter()
                .map(|c| c.name.clone())
                .collect();
            let map: BTreeMap<
                String,
                (
                    sound::CompiledWorldUnit,
                    phera_bytecode::facts::UnitBuildFacts,
                ),
            > = cli
                .units
                .into_iter()
                .zip(cli.unit_facts)
                .map(|(cu, uf)| (cu.file.relative_path.clone(), (cu, uf)))
                .collect();
            (map, names)
        }
        None => (BTreeMap::new(), Vec::new()),
    };

    // `close_and_analyze` emits units sorted by `relative_path`, so each unit's
    // `UnitRef` index matches its final `FILES` index without re-indexing.
    if !analyzed
        .units
        .windows(2)
        .all(|pair| pair[0].file.relative_path <= pair[1].file.relative_path)
    {
        return Err("internal error: analyzed units are not sorted by relative path".into());
    }

    let web_class_index: BTreeMap<&str, phera_bytecode::facts::ClassRef> = analyzed
        .program_facts
        .classes
        .iter()
        .enumerate()
        .map(|(idx, layout)| {
            (
                layout.name.as_ref(),
                phera_bytecode::facts::ClassRef(idx as u32),
            )
        })
        .collect();

    let mut units: Vec<Arc<phera_bytecode::CompiledUnit>> = Vec::new();
    let mut files: Vec<ImageFileEntry> = Vec::with_capacity(analyzed.units.len());
    let mut entry_paths: BTreeSet<String> = BTreeSet::new();

    for (file_idx, (compiled, unit_facts)) in analyzed
        .units
        .into_iter()
        .zip(analyzed.unit_facts)
        .enumerate()
    {
        if compiled.file.is_entry {
            entry_paths.insert(compiled.file.relative_path.clone());
        }

        let mut primary = (*compiled.unit).clone();
        primary.facts = Some(Arc::new(unit_facts));
        let primary_id = units.len() as u32;
        units.push(Arc::new(primary));

        let (unit_web, unit_cli) = match target {
            BuildTarget::Web => (Some(primary_id), None),
            BuildTarget::Cli => (None, Some(primary_id)),
            BuildTarget::Both => {
                let (cli_compiled, mut cli_facts) = cli_by_rel
                    .remove(&compiled.file.relative_path)
                    .ok_or_else(|| {
                        format!(
                            "CLI target build missed file `{}` present in Web target",
                            compiled.file.relative_path
                        )
                    })?;
                if cli_compiled.source_hash != compiled.source_hash {
                    return Err(format!(
                        "`{}` changed on disk during build; retry `phera build`",
                        compiled.file.relative_path
                    ));
                }
                cli_facts.unit = phera_bytecode::facts::UnitRef(file_idx as u32);
                let mut remapped_classes = Vec::with_capacity(cli_facts.classes.len());
                for class_ref in &cli_facts.classes {
                    let Some(name) = cli_class_names.get(class_ref.0 as usize) else {
                        return Err(format!(
                            "CLI target class ref #{} out of bounds in `{}`",
                            class_ref.0, compiled.file.relative_path
                        ));
                    };
                    let Some(&mapped) = web_class_index.get(name.as_ref()) else {
                        return Err(format!(
                            "CLI target declared class `{name}` absent from Web target in `{}`",
                            compiled.file.relative_path
                        ));
                    };
                    remapped_classes.push(mapped);
                }
                cli_facts.classes = remapped_classes.into_boxed_slice();
                let mut cli_unit = (*cli_compiled.unit).clone();
                cli_unit.facts = Some(Arc::new(cli_facts));
                let cli_id = units.len() as u32;
                units.push(Arc::new(cli_unit));
                (Some(primary_id), Some(cli_id))
            }
        };

        let mut flags = FileFlags::NONE;
        if compiled.file.always_loaded {
            flags = flags.union(FileFlags::ALWAYS_LOADED);
        }
        if compiled.file.is_entry {
            flags = flags.union(FileFlags::ENTRY);
        }

        files.push(ImageFileEntry {
            relative_path: compiled.file.relative_path,
            canonical_path: compiled.file.path.to_string_lossy().into_owned(),
            unit_web,
            unit_cli,
            source_len: compiled.source_len,
            source_hash: compiled.source_hash,
            flags,
        });
    }

    if !cli_by_rel.is_empty() {
        let extra: Vec<&str> = cli_by_rel.keys().map(String::as_str).collect();
        return Err(format!(
            "CLI target build discovered extra file(s) absent from Web target: {}",
            extra.join(", ")
        ));
    }

    let classmap: Vec<ImageClassEntry> = analyzed
        .classmap
        .into_iter()
        .map(|(name, file_index, class_id, class_ref)| ImageClassEntry {
            name,
            file_index,
            class_id: class_id.0,
            class_ref: Some(class_ref.0),
        })
        .collect();

    let audit = analyzed.program_facts.audit.clone();
    let manifest = ImageManifest {
        project_root: project_root.to_string_lossy().into_owned(),
        mode,
        has_web,
        has_cli,
        allow: allow.to_vec(),
        framework: analyzed.world.framework.map(str::to_owned),
        entries: entry_paths.into_iter().collect(),
        include_globs: include_globs.to_vec(),
        exclude_globs: exclude_globs.to_vec(),
        include_dev,
    };

    Ok(ImageInput {
        runtime_abi_hash: phera_runtime::image::runtime_abi_hash(),
        manifest,
        files,
        classmap,
        units,
        program_facts: Some(analyzed.program_facts),
        audit,
    })
}

#[derive(Default)]
struct BuildToml {
    output: Option<PathBuf>,
    mode: Option<BuildMode>,
    entries: Vec<PathBuf>,
    include: Vec<String>,
    exclude: Vec<String>,
    allow: Vec<DynamicKind>,
}

fn read_build_toml(project: &Path) -> Result<BuildToml, String> {
    let (table, path) = crate::debug_settings::read_table(project)?;
    let Some(build) = table.get("build") else {
        return Ok(BuildToml::default());
    };
    let label = path
        .as_deref()
        .unwrap_or_else(|| Path::new("phera.toml"))
        .display()
        .to_string();
    let Some(build) = build.as_table() else {
        return Err(format!("{label}: [build] must be a table"));
    };
    const KNOWN_BUILD_KEYS: &[&str] = &[
        "output", "mode", "entry", "entries", "include", "exclude", "allow",
    ];
    for key in build.keys() {
        if !KNOWN_BUILD_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "{label}: unknown [build] key `{key}` (expected one of: {})",
                KNOWN_BUILD_KEYS.join(", ")
            ));
        }
    }
    let mut cfg = BuildToml::default();
    if let Some(value) = build.get("output") {
        let s = value
            .as_str()
            .ok_or_else(|| format!("{label}: build.output must be a string"))?;
        cfg.output = Some(PathBuf::from(s));
    }
    if let Some(value) = build.get("mode") {
        let s = value
            .as_str()
            .ok_or_else(|| format!("{label}: build.mode must be \"hybrid\" or \"strict\""))?;
        cfg.mode = Some(parse_mode(s).map_err(|e| format!("{label}: {e}"))?);
    }
    if let Some(value) = build.get("entry") {
        let s = value
            .as_str()
            .ok_or_else(|| format!("{label}: build.entry must be a string"))?;
        cfg.entries.push(PathBuf::from(s));
    }
    if let Some(value) = build.get("entries") {
        let list = string_array(&label, "build.entries", value)?;
        cfg.entries.extend(list.into_iter().map(PathBuf::from));
    }
    if let Some(value) = build.get("include") {
        cfg.include = string_array(&label, "build.include", value)?;
    }
    if let Some(value) = build.get("exclude") {
        cfg.exclude = string_array(&label, "build.exclude", value)?;
    }
    if let Some(value) = build.get("allow") {
        for item in string_array(&label, "build.allow", value)? {
            let kind = DynamicKind::parse_allow(&item)
                .ok_or_else(|| format!("{label}: unknown build.allow value `{item}`"))?;
            if !cfg.allow.contains(&kind) {
                cfg.allow.push(kind);
            }
        }
    }
    Ok(cfg)
}

fn string_array(label: &str, key: &str, value: &toml::Value) -> Result<Vec<String>, String> {
    let toml::Value::Array(items) = value else {
        return Err(format!("{label}: {key} must be a list of strings"));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{label}: {key} entries must be strings"))
        })
        .collect()
}

fn parse_mode(value: &str) -> Result<BuildMode, String> {
    match value {
        "hybrid" => Ok(BuildMode::Hybrid),
        "strict" => Ok(BuildMode::Strict),
        _ => Err(format!(
            "Unknown build mode `{value}`. Expected `hybrid` or `strict`."
        )),
    }
}
