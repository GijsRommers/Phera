//! Where a program leaves the closed world: `eval($code)`, `new $class`,
//! `include $file`.
//!
//! The audit reads compiled instructions only. A doc comment or a type the
//! runtime does not enforce is never evidence here.
//!
//! What it cannot see is a dynamic *call* of a function it looks for by
//! name, such as `call_user_func('class_alias', ...)`: sealing names in a
//! strict image also needs the runtime to refuse those.

use phera_bytecode::facts::{
    Assumptions, BlastRadius, BuildMode, DynamicAudit, DynamicFinding, DynamicKind, FunctionRef,
    UnitRef,
};
use phera_bytecode::{CompiledUnit, Function, IncludeKind, Instruction, PathBase, SymbolId};

use super::CompiledWorldUnit;
use super::control::path_of;
use super::hierarchy::Hierarchy;
use super::includes::{self, StaticInclude};

/// Audits every function of every unit. `static_includes` is aligned with
/// `units`.
pub(crate) fn audit(
    units: &[CompiledWorldUnit],
    static_includes: &[Vec<StaticInclude>],
    hierarchy: &Hierarchy,
    mode: BuildMode,
    allow: &[DynamicKind],
) -> DynamicAudit {
    let mut findings = Vec::new();
    for (index, (unit, includes)) in units.iter().zip(static_includes).enumerate() {
        let functions = std::iter::once(&unit.unit.main).chain(&unit.unit.functions);
        for (function_index, function) in functions.enumerate() {
            let mut scan = Scan {
                unit: &unit.unit,
                function,
                at: FunctionRef {
                    unit: UnitRef(index as u32),
                    function: function_index as u32,
                },
                static_includes: includes,
                hierarchy,
                findings: &mut findings,
            };
            scan.run();
        }
    }
    findings.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    let holds = assumptions(mode, allow);
    DynamicAudit {
        findings: findings.into_boxed_slice(),
        holds,
    }
}

fn sort_key(finding: &DynamicFinding) -> (FunctionRef, u32, u8, &str) {
    (
        finding.at,
        finding.line,
        finding.kind as u8,
        &finding.detail,
    )
}

/// What a build may assume about the whole program.
///
/// A hybrid image compiles files from disk at run time, so nothing is
/// sealed. A strict image starts with both `SEALED_NAMES` and
/// `SEALED_HIERARCHY` because the strict loader refuses any `include`
/// outside the image (including stream wrappers) and `ImageSeal` refuses
/// `eval()` and `class_alias()` unless explicitly allowed.
///
/// - Allowing `Eval` opens both names and hierarchy (`Assumptions::NONE`).
/// - Allowing `ClassAlias` opens names (including dynamic
///   `call_user_func('class_alias', ...)`) while keeping `SEALED_HIERARCHY`,
///   since an alias binds an existing class and cannot add a subclass.
/// - `DynamicInclude` (such as Composer's `ClassLoader::includeFile`) is
///   still audited and reported, but remains sealed to image units by the
///   strict `CodeCache` loader.
fn assumptions(mode: BuildMode, allow: &[DynamicKind]) -> Assumptions {
    match mode {
        BuildMode::Hybrid => Assumptions::NONE,
        BuildMode::Strict => {
            if allow.contains(&DynamicKind::Eval) {
                Assumptions::NONE
            } else if allow.contains(&DynamicKind::ClassAlias) {
                Assumptions::SEALED_HIERARCHY
            } else {
                Assumptions::SEALED_NAMES.union(Assumptions::SEALED_HIERARCHY)
            }
        }
    }
}

/// The findings a strict build refuses: `eval()` and `class_alias()`, unless
/// the configuration allows their kind.
///
/// `DynamicInclude` is still audited and reported, but is not rejected in
/// strict mode because the strict image loader already confines includes to
/// units inside the `.phb` image and refuses stream includes. A hybrid build
/// refuses nothing.
pub(crate) fn strict_violations<'a>(
    mode: BuildMode,
    audit: &'a DynamicAudit,
    allow: &[DynamicKind],
) -> Vec<&'a DynamicFinding> {
    if mode == BuildMode::Hybrid {
        return Vec::new();
    }
    audit
        .findings
        .iter()
        .filter(|finding| {
            let blocked_in_strict =
                matches!(finding.kind, DynamicKind::Eval | DynamicKind::ClassAlias);
            blocked_in_strict && !allow.contains(&finding.kind)
        })
        .collect()
}

/// The audit of one function.
struct Scan<'a> {
    unit: &'a CompiledUnit,
    function: &'a Function,
    at: FunctionRef,
    static_includes: &'a [StaticInclude],
    hierarchy: &'a Hierarchy,
    findings: &'a mut Vec<DynamicFinding>,
}

impl Scan<'_> {
    fn run(&mut self) {
        let mut reported_paths = vec![false; self.function.paths.len()];
        for (index, instruction) in self.function.instructions.iter().enumerate() {
            let line = self.function.lines.get(index).copied().unwrap_or(0);
            self.instruction(index as u32, instruction, line);
            // `$$name = 1` is a path whose base is a variable found by name.
            if let Some(path) = path_of(instruction)
                && let Some(named) = self.function.paths.get(path.index())
                && matches!(named.base, PathBase::Named(_))
            {
                reported_paths[path.index()] = true;
                self.variable_variable(line);
            }
        }
        // A path no instruction of this audit knows is still reported,
        // at the line of the function.
        for (path, reported) in self.function.paths.iter().zip(reported_paths) {
            if matches!(path.base, PathBase::Named(_)) && !reported {
                self.variable_variable(self.function.definition_line);
            }
        }
    }

    fn instruction(&mut self, index: u32, instruction: &Instruction, line: u32) {
        match *instruction {
            Instruction::Eval { .. } => {
                self.report(DynamicKind::Eval, BlastRadius::Names, line, "eval()");
            }
            Instruction::Include { kind, .. } => {
                if !includes::is_static(self.static_includes, self.at.function, index) {
                    let detail =
                        format!("{} of a path not known at build time", include_word(kind));
                    self.report(
                        DynamicKind::DynamicInclude,
                        BlastRadius::Names,
                        line,
                        detail,
                    );
                }
            }
            Instruction::NewDynamic { .. } => self.report(
                DynamicKind::DynamicNew,
                BlastRadius::Class,
                line,
                "new with a class name computed at run time",
            ),
            Instruction::StaticCall { .. } => self.report(
                DynamicKind::DynamicStaticCall,
                BlastRadius::Class,
                line,
                "static call with a class or method name computed at run time",
            ),
            Instruction::DynamicMethodCall { .. } => self.report(
                DynamicKind::DynamicMethodCall,
                BlastRadius::Class,
                line,
                "method call with a method name computed at run time",
            ),
            Instruction::BindDynamicGlobal { .. } => self.report(
                DynamicKind::VariableVariable,
                BlastRadius::Function,
                line,
                "global $$name",
            ),
            Instruction::InitCall { function } | Instruction::CallNativeDirect { function, .. } => {
                self.named_call(function, line);
            }
            Instruction::New { class } | Instruction::InitStaticCall { class, .. } => {
                self.reflection(class, instruction, line);
            }
            _ => {}
        }
    }

    /// A call of one of the functions that reach past ordinary code.
    fn named_call(&mut self, function: SymbolId, line: u32) {
        let Some(symbol) = self.unit.symbols.get(function.index()) else {
            return;
        };
        // `extract()` in `namespace App` means `App\extract` only if that
        // exists; either reading reports the call.
        let names = [Some(symbol.name.as_str()), symbol.fallback.as_deref()];
        for name in names.into_iter().flatten() {
            let finding = match name.to_ascii_lowercase().as_str() {
                "class_alias" => Some((DynamicKind::ClassAlias, BlastRadius::Names)),
                "extract" => Some((DynamicKind::Extract, BlastRadius::Function)),
                "compact" => Some((DynamicKind::Compact, BlastRadius::Function)),
                "unserialize" => Some((DynamicKind::Unserialize, BlastRadius::Class)),
                _ => None,
            };
            if let Some((kind, radius)) = finding {
                self.report(kind, radius, line, format!("{}()", kind.as_str()));
                return;
            }
        }
    }

    /// `new ReflectionClass($name)` and `ReflectionMethod::createFromMethodName()`:
    /// reflection reaches classes, methods and properties by name.
    fn reflection(&mut self, class: SymbolId, instruction: &Instruction, line: u32) {
        let Some(symbol) = self.unit.symbols.get(class.index()) else {
            return;
        };
        if !self
            .hierarchy
            .is_reflection(&symbol.name.to_ascii_lowercase())
        {
            return;
        }
        let detail = match instruction {
            Instruction::New { .. } => format!("new {}", symbol.name),
            _ => format!("static call on {}", symbol.name),
        };
        self.report(DynamicKind::Reflection, BlastRadius::Class, line, detail);
    }

    fn variable_variable(&mut self, line: u32) {
        self.report(
            DynamicKind::VariableVariable,
            BlastRadius::Function,
            line,
            "variable found by a name computed at run time",
        );
    }

    fn report(
        &mut self,
        kind: DynamicKind,
        radius: BlastRadius,
        line: u32,
        detail: impl Into<Box<str>>,
    ) {
        self.findings.push(DynamicFinding {
            kind,
            radius,
            at: self.at,
            line,
            detail: detail.into(),
        });
    }
}

fn include_word(kind: IncludeKind) -> &'static str {
    match kind {
        IncludeKind::Include => "include",
        IncludeKind::IncludeOnce => "include_once",
        IncludeKind::Require => "require",
        IncludeKind::RequireOnce => "require_once",
    }
}

#[cfg(test)]
mod tests {
    use super::{audit, strict_violations};
    use crate::sound::declarations::Declarations;
    use crate::sound::hierarchy::Hierarchy;
    use crate::sound::includes::static_includes;
    use crate::sound::test_support::{class, function, symbol, text, unit_of, world_unit};
    use phera_bytecode::facts::{
        Assumptions, BlastRadius, BuildMode, DynamicAudit, DynamicKind, FunctionRef, UnitRef,
    };
    use phera_bytecode::{
        CallResult, ClassBinding, CompiledUnit, ConstId, IncludeKind, Instruction, Path, PathBase,
        PathId, Reg, SymbolId,
    };
    use std::collections::BTreeSet;

    fn run(units: Vec<CompiledUnit>, mode: BuildMode, allow: &[DynamicKind]) -> DynamicAudit {
        let units: Vec<_> = units
            .into_iter()
            .enumerate()
            .map(|(index, unit)| world_unit(&format!("{index}.php"), unit))
            .collect();
        let includes: Vec<_> = units
            .iter()
            .map(|unit| static_includes(&unit.unit))
            .collect();
        let native_functions = BTreeSet::new();
        let native_classes = BTreeSet::from(["reflectionclass".to_owned()]);
        let declarations = Declarations::collect(&units, &native_functions, &native_classes);
        let hierarchy = Hierarchy::build(&units, &declarations, &std::collections::BTreeMap::new());
        audit(&units, &includes, &hierarchy, mode, allow)
    }

    fn kinds(audit: &DynamicAudit) -> Vec<(DynamicKind, BlastRadius, u32)> {
        audit
            .findings
            .iter()
            .map(|finding| (finding.kind, finding.radius, finding.line))
            .collect()
    }

    fn call(symbol: u32) -> [Instruction; 2] {
        [
            Instruction::InitCall {
                function: SymbolId(symbol),
            },
            Instruction::Call {
                dst: Reg(1),
                result: CallResult::Discarded,
            },
        ]
    }

    /// One of every feature the audit knows, each on its own line.
    fn dynamic_unit() -> CompiledUnit {
        let mut instructions = vec![
            Instruction::Eval {
                dst: Reg(1),
                source: Reg(0),
            },
            // include $file;
            Instruction::Include {
                kind: IncludeKind::IncludeOnce,
                dst: Reg(1),
                path: Reg(0),
            },
            // require '/srv/a.php';
            Instruction::LoadConst {
                dst: Reg(2),
                constant: ConstId(0),
            },
            Instruction::Include {
                kind: IncludeKind::Require,
                dst: Reg(1),
                path: Reg(2),
            },
            Instruction::NewDynamic { class: Reg(0) },
            Instruction::StaticCall {
                class: Reg(0),
                method: Reg(1),
                binding: ClassBinding::Explicit,
            },
            Instruction::DynamicMethodCall {
                receiver: Reg(0),
                method: Reg(1),
            },
            Instruction::AssignPath {
                path: PathId(0),
                src: Reg(1),
            },
            Instruction::BindDynamicGlobal { name: Reg(0) },
        ];
        // class_alias, App\extract or extract, compact, unserialize, strlen
        for symbol_index in 0..5 {
            instructions.extend(call(symbol_index));
        }
        instructions.push(Instruction::CallNativeDirect {
            function: SymbolId(3),
            args: Reg(2),
            count: 1,
            dst: Reg(1),
            result: CallResult::Used,
        });
        // new ReflectionClass, new App\Mirror (extends it), new App\Plain
        for symbol_index in 5..8 {
            instructions.push(Instruction::New {
                class: SymbolId(symbol_index),
            });
        }
        instructions.push(Instruction::InitStaticCall {
            class: SymbolId(5),
            method: SymbolId(4),
            binding: ClassBinding::Explicit,
        });
        let mut unit = unit_of(instructions);
        unit.main.lines = (1..=unit.main.instructions.len() as u32).collect();
        unit.main.paths = vec![Path {
            base: PathBase::Named(Reg(0)),
            steps: Vec::new(),
        }];
        unit.constants = vec![text("/srv/a.php")];
        unit.symbols = vec![
            symbol("class_alias", None),
            symbol("app\\extract", Some("extract")),
            symbol("compact", None),
            symbol("unserialize", None),
            symbol("strlen", None),
            symbol("ReflectionClass", None),
            symbol("App\\Mirror", None),
            symbol("App\\Plain", None),
        ];
        unit.classes = vec![
            class("App\\Mirror", Some(SymbolId(5))),
            class("App\\Plain", None),
        ];
        unit
    }

    #[test]
    fn every_dynamic_feature_is_found_with_its_reach() {
        use BlastRadius::{Class, Function, Names};
        let found = run(vec![dynamic_unit()], BuildMode::Hybrid, &[]);
        assert_eq!(
            kinds(&found),
            [
                (DynamicKind::Eval, Names, 1),
                (DynamicKind::DynamicInclude, Names, 2),
                // Line 4 is the static require: no finding.
                (DynamicKind::DynamicNew, Class, 5),
                (DynamicKind::DynamicStaticCall, Class, 6),
                (DynamicKind::DynamicMethodCall, Class, 7),
                (DynamicKind::VariableVariable, Function, 8),
                (DynamicKind::VariableVariable, Function, 9),
                (DynamicKind::ClassAlias, Names, 10),
                (DynamicKind::Extract, Function, 12),
                (DynamicKind::Compact, Function, 14),
                (DynamicKind::Unserialize, Class, 16),
                // Line 18 calls strlen: no finding.
                (DynamicKind::Unserialize, Class, 20),
                (DynamicKind::Reflection, Class, 21),
                (DynamicKind::Reflection, Class, 22),
                // Line 23 is new App\Plain: no finding.
                (DynamicKind::Reflection, Class, 24),
            ]
        );
        assert_eq!(
            &*found.findings[1].detail,
            "include_once of a path not known at build time"
        );
        assert_eq!(&*found.findings[12].detail, "new ReflectionClass");
        assert_eq!(&*found.findings[13].detail, "new App\\Mirror");
        assert_eq!(found.holds, Assumptions::NONE);
        assert!(strict_violations(BuildMode::Hybrid, &found, &[]).is_empty());
    }

    #[test]
    fn findings_are_sorted_by_unit_function_and_line() {
        let eval = Instruction::Eval {
            dst: Reg(1),
            source: Reg(0),
        };
        let mut first = unit_of(vec![Instruction::ReturnNull]);
        first.functions = vec![function(vec![eval, eval])];
        first.functions[0].lines = vec![9, 3];
        let second = unit_of(vec![eval]);
        let found = run(vec![first, second], BuildMode::Hybrid, &[]);
        let places: Vec<(FunctionRef, u32)> = found
            .findings
            .iter()
            .map(|finding| (finding.at, finding.line))
            .collect();
        let at = |unit, function| FunctionRef {
            unit: UnitRef(unit),
            function,
        };
        assert_eq!(places, [(at(0, 1), 3), (at(0, 1), 9), (at(1, 0), 0)]);
    }

    #[test]
    fn a_strict_build_is_sealed_until_a_name_changing_feature_is_allowed() {
        let sealed = Assumptions::SEALED_NAMES.union(Assumptions::SEALED_HIERARCHY);

        // Nothing dynamic: sealed, nothing to refuse.
        let clean = run(
            vec![unit_of(vec![Instruction::ReturnNull])],
            BuildMode::Strict,
            &[],
        );
        assert_eq!(clean.holds, sealed);
        assert!(strict_violations(BuildMode::Strict, &clean, &[]).is_empty());

        // Not allowed: strict mode refuses eval and class_alias. DynamicInclude
        // is still audited, but sealed by the strict loader so not rejected.
        let refused = run(vec![dynamic_unit()], BuildMode::Strict, &[]);
        assert_eq!(refused.holds, sealed);
        let violations: Vec<DynamicKind> = strict_violations(BuildMode::Strict, &refused, &[])
            .iter()
            .map(|finding| finding.kind)
            .collect();
        assert_eq!(violations, [DynamicKind::Eval, DynamicKind::ClassAlias]);

        // Allowing eval accepts it and drops both name and hierarchy seals.
        let allow = [DynamicKind::Eval, DynamicKind::Reflection];
        let allowed = run(vec![dynamic_unit()], BuildMode::Strict, &allow);
        assert_eq!(allowed.holds, Assumptions::NONE);
        let violations: Vec<DynamicKind> = strict_violations(BuildMode::Strict, &allowed, &allow)
            .iter()
            .map(|finding| finding.kind)
            .collect();
        assert_eq!(violations, [DynamicKind::ClassAlias]);

        // Allowing class_alias drops SEALED_NAMES but keeps SEALED_HIERARCHY.
        let alias_allowed = run(
            vec![dynamic_unit()],
            BuildMode::Strict,
            &[DynamicKind::ClassAlias],
        );
        assert_eq!(alias_allowed.holds, Assumptions::SEALED_HIERARCHY);
        let violations: Vec<DynamicKind> = strict_violations(
            BuildMode::Strict,
            &alias_allowed,
            &[DynamicKind::ClassAlias],
        )
        .iter()
        .map(|finding| finding.kind)
        .collect();
        assert_eq!(violations, [DynamicKind::Eval]);

        // Allowing a feature that cannot bring in names keeps the seal.
        let local = unit_of(vec![Instruction::NewDynamic { class: Reg(0) }]);
        let kept = run(vec![local], BuildMode::Strict, &[DynamicKind::DynamicNew]);
        assert_eq!(kept.holds, sealed);
    }
}
