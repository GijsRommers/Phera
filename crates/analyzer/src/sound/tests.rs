//! End-to-end tests of `discover_initial` and `close_and_analyze` on real
//! directories, with a stand-in for the compiler.
//!
//! A test file is a list of statements, one per line, that the stand-in
//! turns into the instructions the real compiler emits for them:
//!
//! ```text
//! require /lib/a.php        require __DIR__ . '/lib/a.php';
//! include-computed          include $file;
//! eval                      eval($code);
//! class App\A extends B     class A extends B {}   (in namespace App)
//! function helper           function helper() {}
//! broken                    a syntax error
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use phera_bytecode::facts::{
    Assumptions, BuildMode, ClassFlags, ClassRef, DeclUniqueness, DynamicKind, FunctionRef, UnitRef,
};
use phera_bytecode::{
    ClassId, CompiledUnit, ConstId, FunctionId, IncludeKind, Instruction, Reg, SymbolId,
};

use super::test_support::{class, function, named, symbol, text, unit_of};
use super::{
    AnalyzedWorld, BuildConfig, CompileFailure, audit_report_json, audit_report_text,
    close_and_analyze, discover_initial, strict_violations,
};

fn compile(path: &Path, bytes: &[u8]) -> Result<CompiledUnit, String> {
    let directory = path.parent().unwrap().to_string_lossy().into_owned();
    let source = std::str::from_utf8(bytes).unwrap();
    let mut unit = unit_of(Vec::new());
    unit.file = path.to_string_lossy().into_owned();
    let mut code = Vec::new();
    let mut lines = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let line = line.trim();
        let (statement, rest) = line.split_once(' ').unwrap_or((line, ""));
        let before = code.len();
        match statement {
            "" => {}
            "require" => {
                let constant = ConstId(unit.constants.len() as u32);
                unit.constants.push(text(&format!("{directory}{rest}")));
                code.push(Instruction::LoadConst {
                    dst: Reg(1),
                    constant,
                });
                code.push(Instruction::Include {
                    kind: IncludeKind::Require,
                    dst: Reg(2),
                    path: Reg(1),
                });
            }
            "include-computed" => code.push(Instruction::Include {
                kind: IncludeKind::Include,
                dst: Reg(2),
                path: Reg(0),
            }),
            "eval" => code.push(Instruction::Eval {
                dst: Reg(2),
                source: Reg(0),
            }),
            "class-alias" => {
                let sym = SymbolId(unit.symbols.len() as u32);
                unit.symbols.push(symbol("class_alias", None));
                code.push(Instruction::InitCall { function: sym });
                code.push(Instruction::Call {
                    dst: Reg(2),
                    result: phera_bytecode::CallResult::Discarded,
                });
            }
            "class" => {
                let (name, parent) = match rest.split_once(" extends ") {
                    Some((name, parent)) => {
                        unit.symbols.push(symbol(parent, None));
                        (name, Some(SymbolId(unit.symbols.len() as u32 - 1)))
                    }
                    None => (rest, None),
                };
                unit.classes.push(class(name, parent));
            }
            "function" => {
                unit.hoisted.push(FunctionId(unit.functions.len() as u32));
                unit.functions.push(named(rest, function(Vec::new())));
            }
            "broken" => return Err("syntax error, unexpected end of file".to_owned()),
            other => panic!("the stand-in compiler does not know `{other}`"),
        }
        lines.resize(code.len(), number as u32 + 1);
        debug_assert!(code.len() >= before);
    }
    code.push(Instruction::ReturnNull);
    lines.push(0);
    unit.main.instructions = code.into();
    unit.main.lines = lines;
    Ok(unit)
}

/// Not a real digest: the length and the first bytes are enough to tell
/// the test files apart.
fn hash(bytes: &[u8]) -> [u8; 16] {
    let mut digest = [0; 16];
    digest[0] = bytes.len() as u8;
    for (slot, byte) in digest[1..].iter_mut().zip(bytes) {
        *slot = *byte;
    }
    digest
}

fn write(root: &Path, name: &str, contents: &str) {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, contents).unwrap();
}

fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    for (name, contents) in files {
        write(&root, name, contents);
    }
    (directory, root)
}

fn config(root: &Path, mode: BuildMode) -> BuildConfig {
    BuildConfig {
        project_root: root.to_path_buf(),
        mode,
        entries: Vec::new(),
        include_globs: Vec::new(),
        exclude_globs: Vec::new(),
        include_dev: false,
        allow: Vec::new(),
        native_functions: BTreeSet::new(),
        native_classes: BTreeSet::from(["countable".to_owned()]),
        native_supertypes: std::collections::BTreeMap::new(),
    }
}

fn analyze(config: &BuildConfig) -> Result<AnalyzedWorld, String> {
    let world = discover_initial(config)?;
    close_and_analyze(config, world, compile, hash)
}

fn unit_paths(analyzed: &AnalyzedWorld) -> Vec<&str> {
    analyzed
        .units
        .iter()
        .map(|unit| unit.file.relative_path.as_str())
        .collect()
}

#[test]
fn static_includes_are_followed_until_nothing_is_new() {
    let (_directory, root) = project(&[
        ("index.php", "require /lib/a.php\nrequire /lib/secret.php\n"),
        // a and b require each other; b also names a file that is not there.
        ("lib/a.php", "require /b.php\n"),
        (
            "lib/b.php",
            "require /a.php\nrequire /gone.php\nrequire /../outside.php\n",
        ),
        ("lib/secret.php", ""),
        ("lib/unreferenced.php", ""),
        ("outside.php", "include-computed\n"),
    ]);
    let config = BuildConfig {
        exclude_globs: vec!["lib/secret.php".into()],
        ..config(&root, BuildMode::Hybrid)
    };
    let analyzed = analyze(&config).unwrap();
    assert_eq!(
        unit_paths(&analyzed),
        ["index.php", "lib/a.php", "lib/b.php", "outside.php"]
    );
    // The world lists what was built, in the same order.
    let world_paths: Vec<&str> = analyzed
        .world
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    assert_eq!(world_paths, unit_paths(&analyzed));
    assert_eq!(
        &*analyzed.program_facts.units,
        [
            "index.php".into(),
            "lib/a.php".into(),
            "lib/b.php".into(),
            "outside.php".into()
        ]
    );

    let index = &analyzed.units[0];
    assert!(index.file.is_entry);
    assert_eq!(index.file.path, root.join("index.php"));
    let source = "require /lib/a.php\nrequire /lib/secret.php\n";
    assert_eq!(index.source_len, source.len() as u64);
    assert_eq!(index.source_hash, hash(source.as_bytes()));
    assert!(!analyzed.units[1].file.is_entry);

    // Every require names its file, found or not, so only the computed
    // include of outside.php is dynamic.
    let findings = &analyzed.program_facts.audit.findings;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, DynamicKind::DynamicInclude);
    assert_eq!(
        findings[0].at,
        FunctionRef {
            unit: UnitRef(3),
            function: 0
        }
    );
    assert_eq!(findings[0].line, 1);
    assert!(analyzed.compile_failures.is_empty());
}

/// Stands in for `include 'name.php';`: a path with no directory.
fn compile_relative_include(path: &Path, bytes: &[u8]) -> Result<CompiledUnit, String> {
    let mut unit = compile(path, b"")?;
    if let Some(name) = std::str::from_utf8(bytes)
        .unwrap()
        .strip_prefix("relative ")
    {
        unit.constants = vec![text(name.trim())];
        unit.main.instructions = vec![
            Instruction::LoadConst {
                dst: Reg(1),
                constant: ConstId(0),
            },
            Instruction::Include {
                kind: IncludeKind::Include,
                dst: Reg(2),
                path: Reg(1),
            },
        ]
        .into();
        unit.main.lines = vec![1, 1];
    }
    Ok(unit)
}

#[test]
fn a_relative_include_takes_the_file_beside_it_and_the_one_from_the_root() {
    let (_directory, root) = project(&[
        ("index.php", "relative lib/tool.php\n"),
        ("lib/tool.php", "relative helper.php\n"),
        ("lib/helper.php", ""),
        ("helper.php", ""),
        ("lib/lib/tool.php", ""),
    ]);
    let config = config(&root, BuildMode::Hybrid);
    let world = discover_initial(&config).unwrap();
    let analyzed = close_and_analyze(&config, world, compile_relative_include, hash).unwrap();
    // Which helper.php runs depends on the request's working directory.
    assert_eq!(
        unit_paths(&analyzed),
        ["helper.php", "index.php", "lib/helper.php", "lib/tool.php"]
    );
    assert!(analyzed.program_facts.audit.findings.is_empty());
}

#[test]
fn a_file_that_does_not_compile_is_left_out_and_reported() {
    let (_directory, root) = project(&[
        ("index.php", "require /lib/bad.php\nrequire /lib/good.php\n"),
        ("lib/bad.php", "broken\n"),
        ("lib/good.php", "class Good\n"),
    ]);
    let analyzed = analyze(&config(&root, BuildMode::Hybrid)).unwrap();
    assert_eq!(unit_paths(&analyzed), ["index.php", "lib/good.php"]);
    assert_eq!(
        analyzed.compile_failures,
        [CompileFailure {
            relative_path: "lib/bad.php".into(),
            message: "syntax error, unexpected end of file".into(),
        }]
    );
    assert!(audit_report_text(&analyzed).contains("Not compiled: 1\n  lib/bad.php: syntax error"));
}

#[test]
fn an_entry_or_an_always_loaded_file_that_does_not_compile_is_an_error() {
    let (_directory, root) = project(&[("index.php", "broken\n")]);
    let error = analyze(&config(&root, BuildMode::Hybrid)).unwrap_err();
    assert_eq!(error, "index.php: syntax error, unexpected end of file");

    let (_directory, root) = project(&[
        ("index.php", ""),
        ("vendor/acme/boot.php", "broken\n"),
        (
            "vendor/composer/autoload_files.php",
            "<?php $vendorDir = dirname(__DIR__); return array('a' => $vendorDir . '/acme/boot.php');",
        ),
    ]);
    // The stand-in compiler is also handed Composer's own data file.
    let tolerant = |path: &Path, bytes: &[u8]| {
        if path.ends_with("autoload_files.php") {
            Ok(unit_of(Vec::new()))
        } else {
            compile(path, bytes)
        }
    };
    let config = config(&root, BuildMode::Hybrid);
    let world = discover_initial(&config).unwrap();
    let error = close_and_analyze(&config, world, tolerant, hash).unwrap_err();
    assert_eq!(
        error,
        "vendor/acme/boot.php: syntax error, unexpected end of file"
    );
}

/// index.php   class App\Base / class App\Leaf extends App\Base / function boot
/// lib/more.php   class App\Other extends Countable / class app\leaf (again)
fn two_files() -> (tempfile::TempDir, PathBuf) {
    project(&[
        (
            "index.php",
            "require /lib/more.php\nclass App\\Base\nclass App\\Leaf extends App\\Base\nfunction boot\n",
        ),
        (
            "lib/more.php",
            "class App\\Other extends Countable\nclass app\\leaf\nfunction App\\helper\n",
        ),
    ])
}

#[test]
fn units_classes_functions_and_facts_refer_to_each_other() {
    let (_directory, root) = two_files();
    let analyzed = analyze(&config(&root, BuildMode::Strict)).unwrap();
    let facts = &analyzed.program_facts;
    assert_eq!(facts.mode, BuildMode::Strict);
    assert_eq!(unit_paths(&analyzed), ["index.php", "lib/more.php"]);

    let class_names: Vec<&str> = facts.classes.iter().map(|class| &*class.name).collect();
    assert_eq!(
        class_names,
        ["app\\base", "app\\leaf", "app\\other", "countable"]
    );
    let function_names: Vec<&str> = facts.functions.iter().map(|(name, _, _)| &**name).collect();
    assert_eq!(function_names, ["app\\helper", "boot"]);
    assert_eq!(
        facts.functions[1],
        (
            "boot".into(),
            DeclUniqueness::Unique {
                unit: UnitRef(0),
                hoisted: true
            },
            FunctionRef {
                unit: UnitRef(0),
                function: 1
            }
        )
    );

    // One entry per named declaration: the class declared twice has two.
    assert_eq!(
        analyzed.classmap,
        [
            ("app\\base".to_owned(), 0, ClassId(0), ClassRef(0)),
            ("app\\leaf".to_owned(), 0, ClassId(1), ClassRef(1)),
            ("app\\leaf".to_owned(), 1, ClassId(1), ClassRef(1)),
            ("app\\other".to_owned(), 1, ClassId(0), ClassRef(2)),
        ]
    );
    assert_eq!(facts.classes[1].decl, DeclUniqueness::Duplicate);
    assert_eq!(facts.descendants_of(ClassRef(0)), [ClassRef(1)]);
    assert_eq!(facts.descendants_of(ClassRef(3)), [ClassRef(2)]);

    // Unit facts are aligned with the units and shaped like them.
    assert_eq!(analyzed.unit_facts.len(), 2);
    for (index, (unit, unit_facts)) in analyzed.units.iter().zip(&analyzed.unit_facts).enumerate() {
        assert_eq!(unit_facts.unit, UnitRef(index as u32));
        assert_eq!(unit_facts.functions.len(), 1 + unit.unit.functions.len());
        assert_eq!(unit_facts.classes.len(), unit.unit.classes.len());
    }
    assert_eq!(&*analyzed.unit_facts[1].classes, [ClassRef(2), ClassRef(1)]);

    // Strict and nothing dynamic: the program is sealed, so a concrete class
    // nothing extends is closed.
    let sealed = Assumptions::SEALED_NAMES.union(Assumptions::SEALED_HIERARCHY);
    assert_eq!(facts.audit.holds, sealed);
    assert!(strict_violations(BuildMode::Strict, &facts.audit, &[]).is_empty());
    assert!(
        facts.classes[2]
            .flags
            .contains(ClassFlags::EFFECTIVELY_FINAL)
    );
    assert!(
        !facts.classes[0]
            .flags
            .contains(ClassFlags::EFFECTIVELY_FINAL)
    );
}

#[test]
fn a_hybrid_build_of_the_same_files_assumes_nothing() {
    let (_directory, root) = two_files();
    let analyzed = analyze(&config(&root, BuildMode::Hybrid)).unwrap();
    let facts = &analyzed.program_facts;
    assert_eq!(facts.audit.holds, Assumptions::NONE);
    assert!(
        facts
            .classes
            .iter()
            .all(|class| !class.flags.contains(ClassFlags::EFFECTIVELY_FINAL))
    );
}

#[test]
fn the_same_input_gives_the_same_reports() {
    let (_directory, root) = project(&[
        ("public/index.php", "require /../src/run.php\neval\n"),
        (
            "src/run.php",
            "include-computed\nclass-alias\nclass Runner\n",
        ),
        ("bootstrap/app.php", ""),
        ("artisan", "eval\n"),
    ]);
    let config = BuildConfig {
        allow: vec![DynamicKind::Eval],
        ..config(&root, BuildMode::Strict)
    };
    let analyzed = analyze(&config).unwrap();
    let report = audit_report_text(&analyzed);
    assert_eq!(
        report,
        "Closed-world audit: strict build of 4 files (laravel)\n\
         \x20 classes 1, functions 0\n\
         \x20 holds: nothing (code outside the build can be loaded at run time)\n\
         \n\
         Dynamic features: 4\n\
         \x20 eval                     2  names\n\
         \x20 dynamic_include          1  names\n\
         \x20 class_alias              1  names\n\
         \n\
         Findings\n\
         \x20 artisan:1  eval [names]  {main}: eval()\n\
         \x20 public/index.php:2  eval [names]  {main}: eval()\n\
         \x20 src/run.php:1  dynamic_include [names]  {main}: include of a path not known at build time\n\
         \x20 src/run.php:2  class_alias [names]  {main}: class_alias()\n"
    );
    // The allowed eval is accepted; dynamic_include is sealed by the strict
    // loader; class_alias is still refused unless allowed.
    let refused = strict_violations(
        BuildMode::Strict,
        &analyzed.program_facts.audit,
        &config.allow,
    );
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].kind, DynamicKind::ClassAlias);

    let json: serde_json::Value = serde_json::from_str(&audit_report_json(&analyzed)).unwrap();
    assert_eq!(json["mode"], "strict");
    assert_eq!(json["framework"], "laravel");
    assert_eq!(json["files"], 4);
    assert_eq!(json["holds"]["sealed_names"], false);
    assert_eq!(json["summary"]["eval"], 2);
    assert_eq!(json["findings"][2]["file"], "src/run.php");
    assert_eq!(json["findings"][2]["kind"], "dynamic_include");
    assert_eq!(json["findings"][2]["radius"], "names");
    assert_eq!(json["findings"][2]["function"], "{main}");
    assert_eq!(json["findings"][2]["line"], 1);

    let again = analyze(&config).unwrap();
    assert_eq!(audit_report_text(&again), report);
    assert_eq!(audit_report_json(&again), audit_report_json(&analyzed));
    assert_eq!(again.program_facts, analyzed.program_facts);
    assert_eq!(again.unit_facts, analyzed.unit_facts);
    assert_eq!(again.classmap, analyzed.classmap);
}

#[test]
fn build_time_layout_flow_and_resolution_populate_sound_facts_with_real_compiler() {
    use phera_bytecode::facts::{
        CallFlags, ClassFlags, FunctionFlags, PropAccess, PropFlags, SoundMask, SoundType,
    };

    let (_dir, root) = project(&[
        (
            "src/model.php",
            r#"<?php
namespace Demo;

trait HasId {
    public int $id = 1;
    public function tag(): string { return 'trait'; }
}

class ParentBox {
    private int $secret = 1;
    protected int $count = 2;
    public function baseOnly(): int { return $this->count; }
}

final class Box extends ParentBox {
    use HasId;
    private string $secret = 'x';
    public int $count = 3;
    public function readAll(): int {
        return $this->count;
    }
}

function exact_id(int $n): int {
    return $n;
}

function weak_float(int $n): float {
    return $n;
}

function hot_loop(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum = (int)($sum + $i);
    }
    return $sum;
}
"#,
        ),
        (
            "index.php",
            r#"<?php
require __DIR__ . '/src/model.php';

function entry_helper(int $a): int {
    $b = new Demo\Box();
    $x = $b->readAll();
    $y = Demo\exact_id($a);
    return (int)($x + $y + $b->id);
}
"#,
        ),
    ]);

    let cfg = config(&root, BuildMode::Strict);
    let world = discover_initial(&cfg).unwrap();
    let analyzed = close_and_analyze(
        &cfg,
        world,
        |path, bytes| {
            let src = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
            phera_compiler::compile(src, &path.to_string_lossy()).map_err(|e| e.to_string())
        },
        hash,
    )
    .unwrap();

    // 1. Exact property layout and inherited + trait method table on Demo\Box.
    let box_layout = analyzed
        .program_facts
        .classes
        .iter()
        .find(|c| &*c.name == "demo\\box")
        .expect("demo\\box");
    assert!(box_layout.flags.contains(ClassFlags::LAYOUT_EXACT));
    let keys: Vec<&[u8]> = box_layout.slots.iter().map(|s| &*s.key).collect();
    assert_eq!(
        keys,
        vec![
            &b"\0Demo\\ParentBox\0secret"[..],
            &b"count"[..],
            &b"\0Demo\\Box\0secret"[..],
            &b"id"[..],
        ]
    );
    assert_eq!(box_layout.slots[1].ty, SoundType::INT);
    assert!(box_layout.slots[1].flags.contains(PropFlags::PUBLIC));
    assert!(box_layout.slots[1].flags.contains(PropFlags::TYPED));
    let method_names: Vec<&str> = box_layout.methods.iter().map(|m| &*m.name).collect();
    assert_eq!(method_names, ["baseonly", "readall", "tag"]);

    // 2. `RETURN_PROVEN`, `FAST_CANDIDATE`, and sound return types on src/model.php.
    let model_idx = analyzed
        .units
        .iter()
        .position(|u| u.file.relative_path == "src/model.php")
        .unwrap();
    let model_unit = &analyzed.units[model_idx].unit;
    let model_facts = &analyzed.unit_facts[model_idx];
    let fn_facts = |name: &str| {
        let pos = model_unit
            .functions
            .iter()
            .position(|f| f.name.eq_ignore_ascii_case(name))
            .unwrap();
        &model_facts.functions[pos + 1]
    };
    assert!(
        fn_facts("Demo\\exact_id")
            .flags
            .contains(FunctionFlags::RETURN_PROVEN)
    );
    assert_eq!(fn_facts("Demo\\exact_id").returns, SoundType::INT);
    assert!(
        !fn_facts("Demo\\weak_float")
            .flags
            .contains(FunctionFlags::RETURN_PROVEN)
    );
    assert_eq!(fn_facts("Demo\\weak_float").returns, SoundType::FLOAT);
    assert!(
        fn_facts("Demo\\hot_loop")
            .flags
            .contains(FunctionFlags::FAST_CANDIDATE)
    );
    assert!(
        fn_facts("Demo\\hot_loop")
            .flags
            .contains(FunctionFlags::RETURN_PROVEN)
    );

    // 3. Cross-file resolved calls and resolved property slots in index.php::entry_helper.
    let index_idx = analyzed
        .units
        .iter()
        .position(|u| u.file.relative_path == "index.php")
        .unwrap();
    let index_unit = &analyzed.units[index_idx].unit;
    let helper_pos = index_unit
        .functions
        .iter()
        .position(|f| f.name == "entry_helper")
        .unwrap();
    let helper_facts = &analyzed.unit_facts[index_idx].functions[helper_pos + 1];
    assert!(helper_facts.flags.contains(FunctionFlags::RETURN_PROVEN));
    assert_eq!(helper_facts.returns.mask, SoundMask::INT);
    assert!(
        helper_facts
            .calls
            .iter()
            .any(|c| c.flags.contains(CallFlags::ARGS_EXACT)
                && c.flags.contains(CallFlags::RETURN_PROVEN)
                && c.result == SoundType::INT),
        "expected resolved exact cross-file call in entry_helper: {:?}",
        helper_facts.calls
    );
    assert!(
        helper_facts.props.iter().any(|p| p.slot == 3
            && p.flags.contains(PropAccess::READ_EXACT)
            && p.ty == SoundType::INT),
        "expected resolved slot 3 ($b->id) in entry_helper: {:?}",
        helper_facts.props
    );
}
