//! `require __DIR__ . '/vendor/autoload.php';` names its file at compile
//! time. Such includes are followed until no new file turns up, so that a
//! build holds every file its code can be seen to load.
//!
//! The path is read from the compiled instructions, not from the source:
//!
//! ```text
//! LoadConst r11, "/srv/app/public"      __DIR__, folded by the compiler
//! LoadConst r12, "/../vendor/autoload.php"
//! Binary    Concat r11, r11, r12
//! Include   Require r12, path r11
//! ```
//!
//! Only a straight run of such instructions is trusted. Anything else in
//! between (a call, a jump arriving, another include, which shares the
//! variables of this scope) forgets what was known, and the include is
//! then reported as dynamic by the audit.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use phera_bytecode::{
    BinaryOp, CompiledUnit, ConstId, Constant, Function, Instruction, Reg, SymbolId,
};

use super::control::jump_entries;
use super::discover::sort_files;
use super::glob::GlobSet;
use super::{CompileFailure, CompiledWorldUnit, DiscoveredFile, DiscoveredWorld, relative_path};

/// An `Include` instruction whose path is a compile-time string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticInclude {
    /// 0 is `{main}`; `1 + i` is `unit.functions[i]`.
    pub(crate) function: u32,
    /// The instruction's index in that function.
    pub(crate) at: u32,
    pub(crate) path: Vec<u8>,
}

/// The static includes of a unit, sorted by function and instruction.
pub(crate) fn static_includes(unit: &CompiledUnit) -> Vec<StaticInclude> {
    let mut found = Vec::new();
    let functions = std::iter::once(&unit.main).chain(&unit.functions);
    for (index, function) in functions.enumerate() {
        let has_include = function
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Include { .. }));
        if has_include {
            Tracer::new(unit, function).run(index as u32, &mut found);
        }
    }
    found
}

/// Whether `Include` number `at` of function `function` is a static one.
pub(crate) fn is_static(includes: &[StaticInclude], function: u32, at: u32) -> bool {
    includes
        .binary_search_by_key(&(function, at), |include| (include.function, include.at))
        .is_ok()
}

/// What a register is known to hold at one point of a straight run.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Known {
    Text(Vec<u8>),
    /// A small integer: the levels of `dirname($path, 2)`, or a number
    /// that `.` turns into its decimal digits.
    Int(i64),
}

impl Known {
    /// The bytes PHP's `.` operator appends for this value.
    fn into_text(self) -> Vec<u8> {
        match self {
            Known::Text(bytes) => bytes,
            Known::Int(value) => value.to_string().into_bytes(),
        }
    }
}

/// Follows one function's instructions in order, remembering constants.
struct Tracer<'a> {
    unit: &'a CompiledUnit,
    function: &'a Function,
    registers: Vec<Option<Known>>,
    /// The arguments sent so far to each `dirname(...)` call being prepared,
    /// innermost last: `dirname(dirname(__DIR__))` nests.
    dirname_calls: Vec<Vec<Option<Known>>>,
}

impl<'a> Tracer<'a> {
    fn new(unit: &'a CompiledUnit, function: &'a Function) -> Self {
        Self {
            unit,
            function,
            registers: vec![None; usize::from(function.registers)],
            dirname_calls: Vec::new(),
        }
    }

    fn run(mut self, function_index: u32, found: &mut Vec<StaticInclude>) {
        let entries = jump_entries(self.function);
        for (at, instruction) in self.function.instructions.iter().enumerate() {
            if entries[at] {
                self.forget_everything();
            }
            if let Instruction::Include { path, .. } = *instruction {
                if let Some(Known::Text(text)) = self.get(path)
                    && !is_stream_path(&text)
                {
                    found.push(StaticInclude {
                        function: function_index,
                        at: at as u32,
                        path: text,
                    });
                }
                // The included file runs in this scope and can assign any
                // variable of it.
                self.forget_everything();
            } else {
                self.step(instruction);
            }
        }
    }

    fn step(&mut self, instruction: &Instruction) {
        match *instruction {
            Instruction::Nop => {}
            Instruction::LoadConst { dst, constant } => self.set(dst, self.constant(constant)),
            Instruction::AssignConst { local, constant } => {
                self.set(local, self.constant(constant));
            }
            Instruction::LoadInt { dst, value } => {
                self.set(dst, Some(Known::Int(i64::from(value))));
            }
            Instruction::LoadNull { dst } | Instruction::LoadBool { dst, .. } => {
                self.set(dst, None);
            }
            Instruction::Clear { reg } => self.set(reg, None),
            Instruction::Move { dst, src } => self.set(dst, self.get(src)),
            Instruction::Assign { local, src } => self.set(local, self.get(src)),
            Instruction::AssignTake { local, src } => {
                let value = self.take(src);
                self.set(local, value);
            }
            Instruction::Binary {
                op: BinaryOp::Concat,
                dst,
                lhs,
                rhs,
            } => self.set(dst, concat(self.get(lhs), self.get(rhs))),
            Instruction::AssignOp {
                op: BinaryOp::Concat,
                local,
                src,
            } => self.set(local, concat(self.get(local), self.get(src))),
            // Reading a constant runs no code, and one constant is the same
            // in every request: `__DIR__ . DIRECTORY_SEPARATOR . 'a.php'`.
            Instruction::FetchConstant { dst, constant } => {
                let separator = self
                    .is_global(constant, "DIRECTORY_SEPARATOR")
                    .then(|| Known::Text(std::path::MAIN_SEPARATOR_STR.as_bytes().to_vec()));
                self.set(dst, separator);
            }
            Instruction::InitCall { function } if self.is_global(function, "dirname") => {
                self.dirname_calls.push(Vec::new());
            }
            Instruction::SendConst { constant } => {
                let value = self.constant(constant);
                self.send(value);
            }
            Instruction::SendValue { src, .. } | Instruction::SendLocal { local: src } => {
                let value = self.get(src);
                self.send(value);
            }
            Instruction::SendTake { src, .. } => {
                let value = self.take(src);
                self.send(value);
            }
            Instruction::Call { dst, .. } => match self.dirname_calls.pop() {
                Some(arguments) => self.set(dst, dirname_call(&arguments)),
                None => self.forget_everything(),
            },
            Instruction::CallNativeDirect {
                function,
                args,
                count,
                dst,
                ..
            } if self.is_global(function, "dirname") => {
                let arguments: Vec<Option<Known>> = (0..count)
                    .map(|offset| self.get(Reg(args.0.saturating_add(offset))))
                    .collect();
                self.set(dst, dirname_call(&arguments));
            }
            // Anything else may run code or write a register this function
            // does not model.
            _ => self.forget_everything(),
        }
    }

    /// One more argument of the innermost `dirname` call. A send that
    /// belongs to any other call ends the straight run.
    fn send(&mut self, value: Option<Known>) {
        match self.dirname_calls.last_mut() {
            Some(arguments) => arguments.push(value),
            None => self.forget_everything(),
        }
    }

    fn get(&self, register: Reg) -> Option<Known> {
        self.registers.get(register.index()).cloned().flatten()
    }

    fn take(&mut self, register: Reg) -> Option<Known> {
        self.registers.get_mut(register.index())?.take()
    }

    fn set(&mut self, register: Reg, value: Option<Known>) {
        if let Some(slot) = self.registers.get_mut(register.index()) {
            *slot = value;
        }
    }

    fn forget_everything(&mut self) {
        self.registers.fill(None);
        self.dirname_calls.clear();
    }

    fn constant(&self, constant: ConstId) -> Option<Known> {
        match self.unit.constants.get(constant.index())? {
            Constant::String(bytes) => Some(Known::Text(bytes.clone())),
            Constant::Int(value) => Some(Known::Int(*value)),
            _ => None,
        }
    }

    /// Whether a symbol is the global function or constant `name`, and
    /// nothing a namespace could put in its place: `dirname()` in
    /// `namespace App` may mean `App\dirname`. The runtime defines both
    /// names this is asked about, so no program can redefine them.
    fn is_global(&self, symbol: SymbolId, name: &str) -> bool {
        self.unit
            .symbols
            .get(symbol.index())
            .is_some_and(|symbol| symbol.fallback.is_none() && symbol.name == name)
    }
}

fn concat(left: Option<Known>, right: Option<Known>) -> Option<Known> {
    let mut text = left?.into_text();
    text.extend(right?.into_text());
    Some(Known::Text(text))
}

/// `dirname($path)` and `dirname($path, $levels)` of known arguments.
fn dirname_call(arguments: &[Option<Known>]) -> Option<Known> {
    let (path, levels) = match arguments {
        [Some(Known::Text(path))] => (path, 1),
        [Some(Known::Text(path)), Some(Known::Int(levels))] => (path, *levels),
        _ => return None,
    };
    dirname(path, levels).map(Known::Text)
}

/// PHP's `dirname` for an absolute path with `/` separators.
fn dirname(path: &[u8], levels: i64) -> Option<Vec<u8>> {
    if !path.starts_with(b"/") || levels < 1 {
        return None;
    }
    let mut directory = path;
    for _ in 0..levels {
        directory = parent(directory);
        if directory == b"/" {
            break;
        }
    }
    Some(directory.to_vec())
}

/// The directory above `path`: trailing slashes do not count as a component.
fn parent(path: &[u8]) -> &[u8] {
    let path = without_trailing_slashes(path);
    match path.iter().rposition(|&byte| byte == b'/') {
        Some(separator) => match without_trailing_slashes(&path[..separator]) {
            [] => b"/",
            directory => directory,
        },
        None => b"/",
    }
}

fn without_trailing_slashes(path: &[u8]) -> &[u8] {
    let end = path
        .iter()
        .rposition(|&byte| byte != b'/')
        .map_or(0, |last| last + 1);
    &path[..end]
}

/// `phar://app.phar/a.php` and `data:text/plain,...` are read through a
/// stream wrapper, not from a file a build can hold.
fn is_stream_path(path: &[u8]) -> bool {
    if path.starts_with(b"data:") {
        return true;
    }
    let Some(scheme_end) = path.windows(3).position(|window| window == b"://") else {
        return false;
    };
    let scheme = &path[..scheme_end];
    scheme.first().is_some_and(u8::is_ascii_alphabetic)
        && scheme
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// The files a static include can mean, of those that exist.
///
/// An absolute path means one file. A relative one depends on the working
/// directory of the request, which a build does not know: PHP looks there
/// first and next to the including file second. Both the project root, the
/// usual working directory, and the including file's directory are taken.
fn include_targets(path: &[u8], including: &Path, root: &Path) -> Vec<PathBuf> {
    let Ok(path) = std::str::from_utf8(path) else {
        return Vec::new();
    };
    let path = Path::new(path);
    let candidates = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else {
        let beside = including.parent().map(|directory| directory.join(path));
        beside.into_iter().chain([root.join(path)]).collect()
    };
    candidates
        .iter()
        .filter_map(|candidate| candidate.canonicalize().ok())
        .filter(|canonical| canonical.is_file())
        .collect()
}

/// A discovered world with every file compiled and every statically
/// included file added.
pub(crate) struct ClosedWorld {
    pub(crate) world: DiscoveredWorld,
    /// Sorted by relative path, like `world.files`, and aligned with it.
    pub(crate) units: Vec<CompiledWorldUnit>,
    /// Aligned with `units`.
    pub(crate) static_includes: Vec<Vec<StaticInclude>>,
    pub(crate) compile_failures: Vec<CompileFailure>,
}

/// Compiles the discovered files and follows their static includes until
/// no new file is found.
///
/// A file that cannot be read or compiled is left out and reported, as PHP
/// itself only fails when such a file is loaded. An entry script or a
/// Composer `autoload.files` entry is always loaded, so there it is an error.
pub(crate) fn close<C, H>(
    mut world: DiscoveredWorld,
    exclude: &GlobSet,
    compile: &C,
    hash_source: &H,
) -> Result<ClosedWorld, String>
where
    C: Fn(&Path, &[u8]) -> Result<CompiledUnit, String>,
    H: Fn(&[u8]) -> [u8; 16],
{
    let root = world.project_root.clone();
    let mut known: BTreeSet<PathBuf> = world.files.iter().map(|file| file.path.clone()).collect();
    let mut pending: VecDeque<DiscoveredFile> = std::mem::take(&mut world.files).into();
    let mut compiled: Vec<(CompiledWorldUnit, Vec<StaticInclude>)> = Vec::new();
    let mut compile_failures = Vec::new();

    while let Some(file) = pending.pop_front() {
        let unit = std::fs::read(&file.path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| Ok((compile(&file.path, &bytes)?, bytes)));
        let (unit, bytes) = match unit {
            Ok(compiled) => compiled,
            Err(message) if file.is_entry || file.always_loaded => {
                return Err(format!("{}: {message}", file.relative_path));
            }
            Err(message) => {
                compile_failures.push(CompileFailure {
                    relative_path: file.relative_path,
                    message,
                });
                continue;
            }
        };
        let includes = static_includes(&unit);
        let targets = includes
            .iter()
            .flat_map(|include| include_targets(&include.path, &file.path, &root));
        for target in targets {
            let relative = relative_path(&root, &target);
            if !exclude.matches(&relative) && known.insert(target.clone()) {
                pending.push_back(DiscoveredFile {
                    path: target,
                    relative_path: relative,
                    always_loaded: false,
                    is_entry: false,
                });
            }
        }
        compiled.push((
            CompiledWorldUnit {
                file,
                source_len: bytes.len() as u64,
                source_hash: hash_source(&bytes),
                unit: Arc::new(unit),
            },
            includes,
        ));
    }

    compiled.sort_by(|(a, _), (b, _)| {
        a.file
            .relative_path
            .cmp(&b.file.relative_path)
            .then_with(|| a.file.path.cmp(&b.file.path))
    });
    let (units, static_includes): (Vec<_>, Vec<_>) = compiled.into_iter().unzip();
    compile_failures.sort();

    // The world describes what was built: files that did not compile leave
    // it, and with them what Composer's tables said about them.
    world.files = units.iter().map(|unit| unit.file.clone()).collect();
    sort_files(&mut world.files);
    let built: BTreeSet<&PathBuf> = world.files.iter().map(|file| &file.path).collect();
    world.classmap.retain(|_, path| built.contains(path));
    world.autoload_files.retain(|path| built.contains(path));
    Ok(ClosedWorld {
        world,
        units,
        static_includes,
        compile_failures,
    })
}

#[cfg(test)]
mod tests {
    use super::{StaticInclude, dirname, is_static, is_stream_path, static_includes};
    use crate::sound::test_support::{function, text, unit_of};
    use phera_bytecode::{
        BinaryOp, CallResult, Constant, IncludeKind, Instruction, Reg, Symbol, SymbolId, Target,
    };

    fn include(path: Reg) -> Instruction {
        Instruction::Include {
            kind: IncludeKind::Require,
            dst: Reg(9),
            path,
        }
    }

    fn load(dst: u16, constant: u32) -> Instruction {
        Instruction::LoadConst {
            dst: Reg(dst),
            constant: phera_bytecode::ConstId(constant),
        }
    }

    fn concat(dst: u16, lhs: u16, rhs: u16) -> Instruction {
        Instruction::Binary {
            op: BinaryOp::Concat,
            dst: Reg(dst),
            lhs: Reg(lhs),
            rhs: Reg(rhs),
        }
    }

    fn paths(includes: &[StaticInclude]) -> Vec<(u32, String)> {
        includes
            .iter()
            .map(|include| {
                (
                    include.at,
                    String::from_utf8_lossy(&include.path).into_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn a_folded_directory_and_a_literal_make_a_static_path() {
        // require __DIR__ . '/vendor/autoload.php';
        let mut unit = unit_of(vec![
            load(1, 0),
            load(2, 1),
            concat(1, 1, 2),
            include(Reg(1)),
            Instruction::Clear { reg: Reg(1) },
            Instruction::ReturnNull,
        ]);
        unit.constants = vec![text("/srv/app"), text("/vendor/autoload.php")];
        let includes = static_includes(&unit);
        assert_eq!(
            paths(&includes),
            [(3, "/srv/app/vendor/autoload.php".into())]
        );
        assert!(is_static(&includes, 0, 3));
        assert!(!is_static(&includes, 0, 2));
        assert!(!is_static(&includes, 1, 3));
    }

    #[test]
    fn a_path_through_locals_and_appends_is_followed() {
        // $base = __DIR__; $base .= '/routes'; include $base . '/web.php';
        let mut unit = unit_of(vec![
            Instruction::AssignConst {
                local: Reg(0),
                constant: phera_bytecode::ConstId(0),
            },
            load(2, 1),
            Instruction::AssignOp {
                op: BinaryOp::Concat,
                local: Reg(0),
                src: Reg(2),
            },
            load(3, 2),
            concat(4, 0, 3),
            Instruction::Move {
                dst: Reg(5),
                src: Reg(4),
            },
            Instruction::AssignTake {
                local: Reg(1),
                src: Reg(5),
            },
            include(Reg(1)),
            // The taken temporary is empty now.
            include(Reg(5)),
        ]);
        unit.constants = vec![text("/srv/app"), text("/routes"), text("/web.php")];
        assert_eq!(
            paths(&static_includes(&unit)),
            [(7, "/srv/app/routes/web.php".into())]
        );
    }

    #[test]
    fn an_include_makes_every_variable_unknown_again() {
        // $base = __DIR__; include $base . '/a.php'; include $base . '/b.php';
        // a.php shares the scope and may have assigned $base.
        let mut unit = unit_of(vec![
            Instruction::AssignConst {
                local: Reg(0),
                constant: phera_bytecode::ConstId(0),
            },
            load(2, 1),
            concat(3, 0, 2),
            include(Reg(3)),
            load(2, 1),
            concat(3, 0, 2),
            include(Reg(3)),
        ]);
        unit.constants = vec![text("/srv/app"), text("/a.php")];
        assert_eq!(
            paths(&static_includes(&unit)),
            [(3, "/srv/app/a.php".into())]
        );
    }

    #[test]
    fn a_call_or_an_arriving_jump_ends_what_is_known() {
        // $path = __DIR__ . '/a.php'; f(); include $path;
        let mut unit = unit_of(vec![
            load(1, 0),
            Instruction::InitCall {
                function: SymbolId(0),
            },
            Instruction::Call {
                dst: Reg(2),
                result: CallResult::Discarded,
            },
            include(Reg(1)),
        ]);
        unit.constants = vec![text("/srv/app/a.php")];
        unit.symbols = vec![Symbol {
            name: "f".into(),
            fallback: None,
            written: "f".into(),
        }];
        assert_eq!(static_includes(&unit), []);

        // include $flag ? '/srv/a.php' : '/srv/b.php';
        let mut unit = unit_of(vec![
            Instruction::JumpIfNot {
                condition: Reg(0),
                target: Target(3),
            },
            load(1, 0),
            Instruction::Jump { target: Target(4) },
            load(1, 1),
            include(Reg(1)),
        ]);
        unit.constants = vec![text("/srv/a.php"), text("/srv/b.php")];
        assert_eq!(static_includes(&unit), []);
    }

    #[test]
    fn the_global_dirname_of_a_known_path_is_computed() {
        // require dirname(__DIR__) . '/vendor/autoload_runtime.php';
        // require dirname(__DIR__, 2) . '/x.php';
        let mut unit = unit_of(vec![
            Instruction::InitCall {
                function: SymbolId(0),
            },
            Instruction::SendConst {
                constant: phera_bytecode::ConstId(0),
            },
            Instruction::Call {
                dst: Reg(1),
                result: CallResult::Used,
            },
            load(2, 1),
            concat(1, 1, 2),
            include(Reg(1)),
            Instruction::LoadInt {
                dst: Reg(4),
                value: 2,
            },
            load(3, 0),
            Instruction::CallNativeDirect {
                function: SymbolId(0),
                args: Reg(3),
                count: 2,
                dst: Reg(5),
                result: CallResult::Used,
            },
            load(2, 2),
            concat(5, 5, 2),
            include(Reg(5)),
            // In a namespace the call may mean App\dirname.
            Instruction::InitCall {
                function: SymbolId(1),
            },
            Instruction::SendConst {
                constant: phera_bytecode::ConstId(0),
            },
            Instruction::Call {
                dst: Reg(1),
                result: CallResult::Used,
            },
            include(Reg(1)),
        ]);
        unit.constants = vec![
            text("/srv/app/public"),
            text("/vendor/autoload_runtime.php"),
            text("/x.php"),
        ];
        unit.symbols = vec![
            Symbol {
                name: "dirname".into(),
                fallback: None,
                written: "dirname".into(),
            },
            Symbol {
                name: "app\\dirname".into(),
                fallback: Some("dirname".into()),
                written: "dirname".into(),
            },
        ];
        assert_eq!(
            paths(&static_includes(&unit)),
            [
                (5, "/srv/app/vendor/autoload_runtime.php".into()),
                (11, "/srv/x.php".into()),
            ]
        );
    }

    #[test]
    fn the_directory_separator_is_known_and_other_constants_are_not() {
        // require __DIR__ . DIRECTORY_SEPARATOR . 'a.php';
        // require __DIR__ . APP_PATH;
        let fetch = |dst, symbol| Instruction::FetchConstant {
            dst: Reg(dst),
            constant: SymbolId(symbol),
        };
        let mut unit = unit_of(vec![
            load(1, 0),
            fetch(2, 0),
            concat(1, 1, 2),
            load(2, 1),
            concat(1, 1, 2),
            include(Reg(1)),
            load(1, 0),
            fetch(2, 1),
            concat(1, 1, 2),
            include(Reg(1)),
            // namespace App; ... DIRECTORY_SEPARATOR may be App\DIRECTORY_SEPARATOR.
            load(1, 0),
            fetch(2, 2),
            concat(1, 1, 2),
            include(Reg(1)),
        ]);
        unit.constants = vec![text("/srv/app"), text("a.php")];
        unit.symbols = vec![
            crate::sound::test_support::symbol("DIRECTORY_SEPARATOR", None),
            crate::sound::test_support::symbol("APP_PATH", None),
            crate::sound::test_support::symbol(
                "App\\DIRECTORY_SEPARATOR",
                Some("DIRECTORY_SEPARATOR"),
            ),
        ];
        let separator = std::path::MAIN_SEPARATOR;
        assert_eq!(
            paths(&static_includes(&unit)),
            [(5, format!("/srv/app{separator}a.php"))]
        );
    }

    #[test]
    fn includes_inside_functions_are_found_with_their_function_number() {
        let mut unit = unit_of(vec![Instruction::ReturnNull]);
        unit.constants = vec![text("/srv/app/helpers.php")];
        unit.functions = vec![
            function(vec![Instruction::ReturnNull]),
            function(vec![load(0, 0), include(Reg(0))]),
        ];
        let includes = static_includes(&unit);
        assert_eq!(includes.len(), 1);
        assert_eq!((includes[0].function, includes[0].at), (2, 1));
    }

    #[test]
    fn a_stream_wrapper_path_is_not_a_file_of_the_build() {
        let mut unit = unit_of(vec![
            load(0, 0),
            include(Reg(0)),
            load(0, 1),
            include(Reg(0)),
        ]);
        unit.constants = vec![
            Constant::String(b"phar://app.phar/a.php".to_vec()),
            Constant::String(b"data:text/plain,<?php echo 1;".to_vec()),
        ];
        assert_eq!(static_includes(&unit), []);
        assert!(is_stream_path(b"php://input"));
        assert!(!is_stream_path(b"/srv/a.php"));
        assert!(!is_stream_path(b"relative/a.php"));
        assert!(!is_stream_path(b"/odd/name://x.php"));
    }

    #[test]
    fn dirname_matches_php_for_absolute_paths() {
        let parent = |path: &str, levels| {
            dirname(path.as_bytes(), levels).map(|bytes| String::from_utf8(bytes).unwrap())
        };
        assert_eq!(parent("/srv/app/public", 1).as_deref(), Some("/srv/app"));
        assert_eq!(parent("/srv/app/public/", 1).as_deref(), Some("/srv/app"));
        assert_eq!(parent("/srv//app", 1).as_deref(), Some("/srv"));
        assert_eq!(parent("/srv", 1).as_deref(), Some("/"));
        assert_eq!(parent("/", 1).as_deref(), Some("/"));
        assert_eq!(parent("/a/b/c", 2).as_deref(), Some("/a"));
        assert_eq!(parent("/a/b/c", 9).as_deref(), Some("/"));
        assert_eq!(parent("relative/path", 1), None);
        assert_eq!(parent("/a/b", 0), None);
    }
}
