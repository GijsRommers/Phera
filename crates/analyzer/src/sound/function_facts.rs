//! What each function's variables can be, as far as its own instructions
//! show: `function f(&$total) { static $calls; ... }` has two variables
//! that are references.
//!
//! A flag here means *maybe*. A variable without `MAYBE_REF` is proven to
//! never hold a PHP reference, so every way a variable becomes one has to be
//! listed in this file:
//!
//! - a by-reference parameter, `global $x`, `static $x`, `$a = &$b`,
//!   `foreach ($list as &$item)`, `function () use (&$x)`;
//! - being passed to a call: the callee decides whether its parameter is
//!   by reference, so only a callee that is known for certain clears this;
//! - a function with dynamic scope (`$$name`, `extract`, `include`, `eval`
//!   and every file's top-level code): all of its variables.

use phera_bytecode::facts::{
    ClassRef, FunctionBuildFacts, FunctionFlags, LocalFlags, UnitBuildFacts, UnitRef,
};
use phera_bytecode::{
    CaptureKind, CompiledUnit, Function, FunctionId, Instruction, IterId, NameId, Parameter,
    PathBase, PathId, Reg, SiteArgument, Symbol,
};

use super::CompiledWorldUnit;
use super::declarations::Declarations;

/// Finds the function a call by name reaches, when only one is possible.
pub(crate) struct Callees<'a> {
    units: &'a [CompiledWorldUnit],
    declarations: &'a Declarations<'a>,
    /// No function can be declared at run time that the build has not seen.
    sealed_names: bool,
}

/// A user function and the unit whose name table spells its parameters.
#[derive(Clone, Copy)]
struct Callee<'a> {
    unit: &'a CompiledUnit,
    function: &'a Function,
}

impl<'a> Callees<'a> {
    pub(crate) fn new(
        units: &'a [CompiledWorldUnit],
        declarations: &'a Declarations<'a>,
        sealed_names: bool,
    ) -> Self {
        Self {
            units,
            declarations,
            sealed_names,
        }
    }

    /// The one function `symbol` can mean.
    ///
    /// `helper()` in `namespace App` means `App\helper` if that is declared
    /// when the call first runs and the global `helper` otherwise. Which of
    /// two existing candidates it is depends on the order files are loaded
    /// in, so such a call has no certain callee.
    fn resolve(&self, symbol: &Symbol) -> Option<Callee<'a>> {
        use phera_bytecode::facts::DeclUniqueness::Unknown;
        if !self.sealed_names {
            return None;
        }
        let name = symbol.name.to_ascii_lowercase();
        let target = match &symbol.fallback {
            None => name,
            Some(fallback) if self.declarations.function(&name) == Unknown => {
                fallback.to_ascii_lowercase()
            }
            Some(_) => return None,
        };
        let at = self.declarations.unique_function(&target)?;
        let unit = &self.units.get(at.unit.0 as usize)?.unit;
        let function = unit.functions.get(at.function.checked_sub(1)? as usize)?;
        Some(Callee { unit, function })
    }
}

/// The facts of one unit. `classes` are the table entries of its class
/// declarations, by `ClassId`.
pub(crate) fn unit_facts(
    unit_ref: UnitRef,
    unit: &CompiledUnit,
    classes: Box<[ClassRef]>,
    callees: &Callees<'_>,
) -> UnitBuildFacts {
    let captures = captured_by_reference(unit);
    let main = function_facts(unit, &unit.main, &Captures::Unsure, callees);
    let functions = unit
        .functions
        .iter()
        .zip(&captures)
        .map(|(function, captures)| function_facts(unit, function, captures, callees));
    UnitBuildFacts {
        unit: unit_ref,
        classes,
        functions: std::iter::once(main).chain(functions).collect(),
    }
}

/// How a closure's captured variables arrive, as the code that creates the
/// closure says: `function () use ($a, &$b)` is `[false, true]`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Captures {
    /// No `MakeClosure` names the function, or two disagree: every captured
    /// variable is taken to be a reference.
    Unsure,
    /// Per captured variable, in register order: captured by reference.
    Known(Vec<bool>),
}

/// For each function of the unit, how its captures are bound.
fn captured_by_reference(unit: &CompiledUnit) -> Vec<Captures> {
    let mut found: Vec<Option<Captures>> = vec![None; unit.functions.len()];
    for body in std::iter::once(&unit.main).chain(&unit.functions) {
        // `Capture` instructions directly precede their `MakeClosure`.
        let mut pending: Vec<(Reg, CaptureKind)> = Vec::new();
        for instruction in &body.instructions {
            match *instruction {
                Instruction::Capture { src, kind } => pending.push((src, kind)),
                Instruction::MakeClosure { function, .. } => {
                    let captures = closure_captures(unit, body, function, &pending);
                    if let Some(slot) = found.get_mut(function.index()) {
                        *slot = Some(match slot.take() {
                            None => captures,
                            Some(earlier) if earlier == captures => captures,
                            Some(_) => Captures::Unsure,
                        });
                    }
                    pending.clear();
                }
                _ => pending.clear(),
            }
        }
    }
    found
        .into_iter()
        .map(|captures| captures.unwrap_or(Captures::Unsure))
        .collect()
}

/// Matches the captures before one `MakeClosure` with the closure's capture
/// registers, which follow its parameters. The count and the variable names
/// must agree, or nothing is concluded.
fn closure_captures(
    unit: &CompiledUnit,
    creator: &Function,
    closure: FunctionId,
    captures: &[(Reg, CaptureKind)],
) -> Captures {
    let Some(closure) = unit.functions.get(closure.index()) else {
        return Captures::Unsure;
    };
    if captures.len() != usize::from(closure.captures) {
        return Captures::Unsure;
    }
    let first = closure.parameters.len();
    let names_agree = captures.iter().enumerate().all(|(index, (source, _))| {
        match (
            creator.locals.get(source.index()),
            closure.locals.get(first + index),
        ) {
            (Some(outside), Some(inside)) => outside == inside,
            // A capture of a temporary has no name to compare.
            (None, Some(_)) => true,
            (_, None) => false,
        }
    });
    if !names_agree {
        return Captures::Unsure;
    }
    Captures::Known(
        captures
            .iter()
            .map(|(_, kind)| *kind == CaptureKind::Reference)
            .collect(),
    )
}

fn function_facts(
    unit: &CompiledUnit,
    function: &Function,
    captures: &Captures,
    callees: &Callees<'_>,
) -> FunctionBuildFacts {
    let by_name = function.dynamic_scope || reaches_variables_by_name(unit, function);
    let mut flags = FunctionFlags::NONE;
    if by_name {
        flags = flags.union(FunctionFlags::DYNAMIC_SCOPE);
    }
    if function.is_generator {
        flags = flags.union(FunctionFlags::IS_GENERATOR);
    }
    let locals = if by_name {
        // Code this function does not contain reads, assigns, unsets and
        // binds its variables by name.
        let anything = LocalFlags::BY_NAME
            .union(LocalFlags::MAYBE_REF)
            .union(LocalFlags::MAYBE_UNDEF);
        vec![anything; function.locals.len()]
    } else {
        LocalScan::new(unit, function, callees).run(captures)
    };
    FunctionBuildFacts {
        flags,
        locals: locals.into_boxed_slice(),
        ..FunctionBuildFacts::default()
    }
}

/// The functions that read or write the variables of the code calling them.
const SCOPE_FUNCTIONS: [&str; 3] = ["extract", "compact", "get_defined_vars"];

/// Whether code outside this function's instructions can reach its
/// variables by name.
///
/// The compiler records that as `dynamic_scope` for the forms it recognises
/// in the source. It looks at a call's written name, so it misses a scope
/// function called under an imported one:
///
/// ```php
/// use function extract as fill;
/// function render(array $data) { fill($data); }
/// ```
///
/// The symbol the call resolved to still says `extract`, which is what is
/// checked here, next to the instructions that are dynamic by themselves.
fn reaches_variables_by_name(unit: &CompiledUnit, function: &Function) -> bool {
    let scope_function = |symbol: &Symbol| {
        [Some(symbol.name.as_str()), symbol.fallback.as_deref()]
            .into_iter()
            .flatten()
            .any(|name| {
                SCOPE_FUNCTIONS
                    .iter()
                    .any(|known| name.eq_ignore_ascii_case(known))
            })
    };
    let named_path = function
        .paths
        .iter()
        .any(|path| matches!(path.base, PathBase::Named(_)));
    named_path
        || function
            .instructions
            .iter()
            .any(|instruction| match *instruction {
                Instruction::Include { .. }
                | Instruction::Eval { .. }
                | Instruction::BindDynamicGlobal { .. } => true,
                Instruction::InitCall { function }
                | Instruction::CallNativeDirect { function, .. } => unit
                    .symbols
                    .get(function.index())
                    .is_some_and(scope_function),
                _ => false,
            })
}

/// A call whose arguments are being sent. Calls nest: `f(g($x))`.
struct OpenCall<'a> {
    /// The function the call reaches, when that is certain.
    callee: Option<Callee<'a>>,
    /// The position the next positional argument is sent to.
    position: usize,
    /// Set by `NameArgument` for the one send that follows it.
    named: Option<NameId>,
    /// `...$arguments` was sent: later positions are not known.
    spread: bool,
}

/// The scan of a function without dynamic scope.
struct LocalScan<'a> {
    unit: &'a CompiledUnit,
    function: &'a Function,
    callees: &'a Callees<'a>,
    maybe_reference: Vec<bool>,
    unset: Vec<bool>,
    calls: Vec<OpenCall<'a>>,
    /// Variables sent to a parameter that is certainly by value. They count
    /// as references after all if the calls of this function do not nest the
    /// way this scan assumes.
    sent_by_value: Vec<Reg>,
    calls_are_bracketed: bool,
}

impl<'a> LocalScan<'a> {
    fn new(unit: &'a CompiledUnit, function: &'a Function, callees: &'a Callees<'a>) -> Self {
        Self {
            unit,
            function,
            callees,
            maybe_reference: vec![false; function.locals.len()],
            unset: vec![false; function.locals.len()],
            calls: Vec::new(),
            sent_by_value: Vec::new(),
            calls_are_bracketed: true,
        }
    }

    fn run(mut self, captures: &Captures) -> Vec<LocalFlags> {
        let parameters = self.function.parameters.len();
        for (index, parameter) in self.function.parameters.iter().enumerate() {
            if parameter.by_reference {
                self.reference(Reg(index as u16));
            }
        }
        for capture in 0..usize::from(self.function.captures) {
            let by_reference = match captures {
                Captures::Known(kinds) => kinds.get(capture).copied().unwrap_or(true),
                Captures::Unsure => true,
            };
            if by_reference {
                self.reference(Reg((parameters + capture) as u16));
            }
        }
        // `$order->total($tax)` in one instruction still binds `$tax` to a
        // by-reference parameter when the method has one.
        for argument in &self.function.site_arguments {
            if let SiteArgument::Local(register) = *argument {
                self.reference(register);
            }
        }

        // Iterators are numbered per loop nesting, and a loop's `IterNext`
        // need not follow its `IterStartReference` in instruction order.
        let reference_iterators: Vec<IterId> = self
            .function
            .instructions
            .iter()
            .filter_map(|instruction| match *instruction {
                Instruction::IterStartReference { iter, .. } => Some(iter),
                _ => None,
            })
            .collect();
        for instruction in &self.function.instructions {
            self.instruction(instruction, &reference_iterators);
        }
        if !self.calls.is_empty() {
            self.calls_are_bracketed = false;
        }
        if !self.calls_are_bracketed {
            for register in std::mem::take(&mut self.sent_by_value) {
                self.reference(register);
            }
        }

        (0..self.function.locals.len())
            .map(|local| {
                let mut flags = LocalFlags::NONE;
                if self.maybe_reference[local] {
                    flags = flags.union(LocalFlags::MAYBE_REF);
                }
                // A parameter is bound before the body runs: by the caller,
                // by its default, or the call fails. Only `unset` undoes that.
                // Any other variable may be read before its first assignment.
                if local >= parameters || self.unset[local] {
                    flags = flags.union(LocalFlags::MAYBE_UNDEF);
                }
                flags
            })
            .collect()
    }

    fn instruction(&mut self, instruction: &Instruction, reference_iterators: &[IterId]) {
        match *instruction {
            // ----- A variable becomes, or is bound to, a reference -----
            Instruction::MakeReference { dst, path } => {
                self.reference(dst);
                self.reference_path(path);
            }
            Instruction::BindReference { path, src } => {
                self.reference_path(path);
                self.reference(src);
            }
            Instruction::BindGlobal { local, .. }
            | Instruction::BindStatic { local, .. }
            | Instruction::InitStatic { local, .. } => self.reference(local),
            Instruction::Capture {
                src,
                kind: CaptureKind::Reference,
            } => self.reference(src),
            Instruction::IterStartReference { subject, path, .. } => {
                self.reference(subject);
                self.reference_path(path);
            }
            Instruction::IterNext { iter, value, .. }
            | Instruction::IterNextKeyed { iter, value, .. } => {
                if reference_iterators.contains(&iter) {
                    self.reference(value);
                }
            }
            Instruction::ReturnReference { src } => self.reference(src),
            Instruction::Yield {
                value: Some(value), ..
            } if self.function.returns_reference => self.reference(value),

            // ----- A variable stops existing -----
            Instruction::UnsetLocal { local } => self.mark_unset(local),
            Instruction::UnsetPath { path } => {
                if let Some(path) = self.function.paths.get(path.index())
                    && let PathBase::Register(register) = path.base
                    && path.steps.is_empty()
                {
                    self.mark_unset(register);
                }
            }

            // ----- Calls: the callee decides how an argument is bound -----
            Instruction::InitCall { function } => {
                let callee = self
                    .unit
                    .symbols
                    .get(function.index())
                    .and_then(|symbol| self.callees.resolve(symbol));
                self.open_call(callee);
            }
            Instruction::InitDynamicCall { .. }
            | Instruction::New { .. }
            | Instruction::NewDynamic { .. }
            | Instruction::InitMethodCall { .. }
            | Instruction::DynamicMethodCall { .. }
            | Instruction::StaticCall { .. }
            | Instruction::InitStaticCall { .. }
            | Instruction::InitUserCallback { .. } => self.open_call(None),
            Instruction::NameArgument { name } => match self.calls.last_mut() {
                Some(call) => call.named = Some(name),
                None => self.calls_are_bracketed = false,
            },
            Instruction::SendLocal { local } => self.send(Some(local)),
            Instruction::SendPath { path } => {
                let base = self
                    .function
                    .paths
                    .get(path.index())
                    .and_then(|path| match path.base {
                        PathBase::Register(register) => Some(register),
                        _ => None,
                    });
                self.send(base);
            }
            Instruction::SendValue { .. }
            | Instruction::SendTake { .. }
            | Instruction::SendConst { .. }
            | Instruction::SendUserCallback { .. } => self.send(None),
            Instruction::SendSpread { .. } => match self.calls.last_mut() {
                Some(call) => call.spread = true,
                None => self.calls_are_bracketed = false,
            },
            Instruction::Call { .. } | Instruction::MakeCallable { .. } => self.close_call(),
            _ => {}
        }
    }

    fn open_call(&mut self, callee: Option<Callee<'a>>) {
        self.calls.push(OpenCall {
            callee,
            position: 0,
            named: None,
            spread: false,
        });
    }

    fn close_call(&mut self) {
        if self.calls.pop().is_none() {
            self.calls_are_bracketed = false;
        }
    }

    /// One argument of the innermost open call; `variable` is the local it
    /// passes, if it passes one.
    fn send(&mut self, variable: Option<Reg>) {
        let Some(call) = self.calls.last_mut() else {
            self.calls_are_bracketed = false;
            if let Some(variable) = variable {
                self.reference(variable);
            }
            return;
        };
        let named = call.named.take();
        let by_value = takes_by_value(self.unit, call, named);
        if named.is_none() {
            call.position += 1;
        }
        match variable {
            Some(variable) if by_value => self.sent_by_value.push(variable),
            Some(variable) => self.reference(variable),
            None => {}
        }
    }

    fn reference(&mut self, register: Reg) {
        if let Some(flag) = self.maybe_reference.get_mut(register.index()) {
            *flag = true;
        }
    }

    /// The variable a path starts at. `$a['x'] = &$b` makes an element of
    /// `$a` a reference, not `$a` itself; it is marked all the same.
    fn reference_path(&mut self, path: PathId) {
        if let Some(path) = self.function.paths.get(path.index())
            && let PathBase::Register(register) = path.base
        {
            self.reference(register);
        }
    }

    fn mark_unset(&mut self, register: Reg) {
        if let Some(flag) = self.unset.get_mut(register.index()) {
            *flag = true;
        }
    }
}

/// Whether the next argument of `call` is certainly bound by value.
/// `caller` spells the name of a named argument.
fn takes_by_value(caller: &CompiledUnit, call: &OpenCall<'_>, named: Option<NameId>) -> bool {
    let Some(callee) = call.callee else {
        return false;
    };
    let parameters = &callee.function.parameters;
    // Arguments past the declared parameters, and unknown named ones, are
    // collected by a variadic parameter if there is one.
    let variadic = parameters.last().filter(|parameter| parameter.variadic);
    let parameter: Option<&Parameter> = match named {
        Some(name) => {
            let Some(name) = caller.names.get(name.index()) else {
                return false;
            };
            parameters
                .iter()
                .find(|parameter| {
                    !parameter.variadic
                        && callee.unit.names.get(parameter.name.index()) == Some(name)
                })
                .or(variadic)
        }
        None if call.spread => return false,
        None => parameters.get(call.position).or(variadic),
    };
    // No parameter takes it: a surplus argument is passed by value, and an
    // unknown named argument makes the call fail.
    parameter.is_none_or(|parameter| !parameter.by_reference)
}

#[cfg(test)]
mod tests {
    use super::{Callees, unit_facts};
    use crate::sound::declarations::Declarations;
    use crate::sound::test_support::{function, named, symbol, unit_of, world_unit};
    use phera_bytecode::facts::{FunctionFlags, LocalFlags, UnitBuildFacts, UnitRef};
    use phera_bytecode::{
        CallResult, CaptureKind, ClosureBinding, CompiledUnit, Function, FunctionId, Instruction,
        IterId, NameId, Parameter, Path, PathBase, PathId, Reg, StaticId, Step, SymbolId, Target,
    };
    use std::collections::BTreeSet;

    const REF: LocalFlags = LocalFlags::MAYBE_REF;
    const UNDEF: LocalFlags = LocalFlags::MAYBE_UNDEF;
    const NONE: LocalFlags = LocalFlags::NONE;

    fn facts(unit: CompiledUnit, sealed_names: bool) -> UnitBuildFacts {
        let units = vec![world_unit("a.php", unit)];
        let native_functions = BTreeSet::from(["count".to_owned()]);
        let native_classes = BTreeSet::new();
        let declarations = Declarations::collect(&units, &native_functions, &native_classes);
        let callees = Callees::new(&units, &declarations, sealed_names);
        unit_facts(UnitRef(0), &units[0].unit, Box::default(), &callees)
    }

    fn parameter(name: u32, by_reference: bool) -> Parameter {
        Parameter {
            default_display: None,
            name: NameId(name),
            ty: None,
            by_reference,
            variadic: false,
            has_default: false,
        }
    }

    /// A function with `locals` named variables, the first of them parameters.
    fn body(locals: u32, parameters: Vec<Parameter>, code: Vec<Instruction>) -> Function {
        Function {
            locals: (0..locals).map(NameId).collect(),
            parameters,
            ..function(code)
        }
    }

    fn locals(facts: &UnitBuildFacts, function: usize) -> Vec<LocalFlags> {
        facts.functions[function].locals.to_vec()
    }

    fn path(base: u16, steps: Vec<Step>) -> Path {
        Path {
            base: PathBase::Register(Reg(base)),
            steps,
        }
    }

    #[test]
    fn top_level_code_and_dynamic_scope_make_every_variable_anything() {
        let mut unit = unit_of(vec![Instruction::ReturnNull]);
        unit.main.locals = vec![NameId(0), NameId(1)];
        let mut generator = body(1, vec![parameter(0, false)], vec![]);
        generator.is_generator = true;
        unit.functions = vec![generator];
        let facts = facts(unit, true);
        assert_eq!(facts.unit, UnitRef(0));
        assert_eq!(facts.functions.len(), 2);
        assert_eq!(facts.functions[0].flags, FunctionFlags::DYNAMIC_SCOPE);
        let anything = LocalFlags::BY_NAME.union(REF).union(UNDEF);
        assert_eq!(locals(&facts, 0), [anything, anything]);
        assert_eq!(facts.functions[1].flags, FunctionFlags::IS_GENERATOR);
        assert_eq!(locals(&facts, 1), [NONE]);
        assert!(facts.functions[1].locals[0].is_proven_non_ref());
    }

    #[test]
    fn a_scope_function_under_an_imported_name_still_reaches_every_variable() {
        // use function extract as fill;
        // function render($data) { fill($data); }     no dynamic scope from the compiler
        // function plain($data) { strlen($data); }
        let call = |symbol| {
            vec![
                Instruction::InitCall {
                    function: SymbolId(symbol),
                },
                Instruction::SendValue {
                    src: Reg(5),
                    of_call: false,
                },
                Instruction::Call {
                    dst: Reg(6),
                    result: CallResult::Discarded,
                },
            ]
        };
        let mut unit = unit_of(vec![]);
        unit.symbols = vec![
            symbol("extract", None),
            symbol("strlen", None),
            symbol("app\\compact", Some("compact")),
        ];
        unit.functions = vec![
            body(1, vec![parameter(0, false)], call(0)),
            body(1, vec![parameter(0, false)], call(1)),
            body(1, vec![parameter(0, false)], call(2)),
        ];
        let facts = facts(unit, true);
        let anything = LocalFlags::BY_NAME.union(REF).union(UNDEF);
        assert_eq!(facts.functions[1].flags, FunctionFlags::DYNAMIC_SCOPE);
        assert_eq!(locals(&facts, 1), [anything]);
        assert_eq!(facts.functions[2].flags, FunctionFlags::NONE);
        assert_eq!(locals(&facts, 2), [NONE]);
        assert_eq!(locals(&facts, 3), [anything]);
    }

    #[test]
    fn every_way_to_make_a_reference_marks_its_variable() {
        // function f($plain, &$out, $gone) {
        //     static $calls; global $config;
        //     $alias = &$target;  $box['k'] = &$other;
        //     foreach ($list as &$item) {}
        //     $closure = function () use (&$shared, $copied) {};
        //     unset($gone);
        //     return $untouched;
        // }
        let mut f = body(
            14,
            vec![parameter(0, false), parameter(1, true), parameter(2, false)],
            vec![
                Instruction::BindStatic {
                    local: Reg(3),
                    variable: StaticId(0),
                    initialized: Target(1),
                },
                Instruction::BindGlobal {
                    local: Reg(4),
                    name: NameId(4),
                },
                Instruction::MakeReference {
                    dst: Reg(20),
                    path: PathId(0),
                },
                Instruction::BindReference {
                    path: PathId(1),
                    src: Reg(20),
                },
                Instruction::MakeReference {
                    dst: Reg(20),
                    path: PathId(2),
                },
                Instruction::BindReference {
                    path: PathId(3),
                    src: Reg(20),
                },
                Instruction::IterStartReference {
                    subject: Reg(21),
                    iter: IterId(0),
                    path: PathId(4),
                    end: Target(9),
                },
                Instruction::IterNext {
                    iter: IterId(0),
                    value: Reg(10),
                    end: Target(9),
                },
                Instruction::Capture {
                    src: Reg(11),
                    kind: CaptureKind::Reference,
                },
                Instruction::Capture {
                    src: Reg(12),
                    kind: CaptureKind::Value,
                },
                Instruction::MakeClosure {
                    dst: Reg(22),
                    function: FunctionId(1),
                    binding: ClosureBinding::Bound,
                },
                Instruction::UnsetLocal { local: Reg(2) },
                Instruction::Return { src: Reg(13) },
            ],
        );
        f.registers = 24;
        f.paths = vec![
            path(6, vec![]),                     // $target
            path(5, vec![]),                     // $alias
            path(8, vec![]),                     // $other
            path(7, vec![Step::Index(Reg(20))]), // $box['k']
            path(9, vec![]),                     // $list
        ];
        // The closure: no parameters, captures $shared (name 11), $copied (name 12).
        let mut closure = body(3, vec![], vec![]);
        closure.locals = vec![NameId(11), NameId(12), NameId(40)];
        closure.captures = 2;
        let mut unit = unit_of(vec![]);
        unit.functions = vec![f, closure];
        let facts = facts(unit, false);
        assert_eq!(
            locals(&facts, 1),
            [
                NONE,             // $plain: a parameter nothing touches
                REF,              // &$out
                UNDEF,            // $gone: unset
                REF.union(UNDEF), // static $calls
                REF.union(UNDEF), // global $config
                REF.union(UNDEF), // $alias
                REF.union(UNDEF), // $target
                REF.union(UNDEF), // $box
                REF.union(UNDEF), // $other
                REF.union(UNDEF), // $list
                REF.union(UNDEF), // $item
                REF.union(UNDEF), // $shared
                UNDEF,            // $copied
                UNDEF,            // $untouched
            ]
        );
        // Inside the closure only the by-reference capture is a reference.
        assert_eq!(locals(&facts, 2), [REF.union(UNDEF), UNDEF, UNDEF]);
    }

    #[test]
    fn captures_are_references_unless_the_creating_code_proves_otherwise() {
        // The count of Capture instructions does not match the closure.
        let creator = body(
            2,
            vec![],
            vec![
                Instruction::Capture {
                    src: Reg(0),
                    kind: CaptureKind::Value,
                },
                Instruction::MakeClosure {
                    dst: Reg(5),
                    function: FunctionId(1),
                    binding: ClosureBinding::Static,
                },
            ],
        );
        let mut closure = body(2, vec![], vec![]);
        closure.captures = 2;
        // Never created by a MakeClosure of this unit.
        let mut orphan = body(1, vec![], vec![]);
        orphan.captures = 1;
        let mut unit = unit_of(vec![]);
        unit.functions = vec![creator, closure, orphan];
        let facts = facts(unit, false);
        assert_eq!(locals(&facts, 2), [REF.union(UNDEF), REF.union(UNDEF)]);
        assert_eq!(locals(&facts, 3), [REF.union(UNDEF)]);
    }

    /// function caller($a, $b, $c, $d, $e) {
    ///     target($a, $b);            // target($x, &$y)
    ///     target(y: $c, x: $d);
    ///     count($e);                 // a function of the runtime
    /// }
    fn calling_unit() -> CompiledUnit {
        let call = |symbol| Instruction::InitCall {
            function: SymbolId(symbol),
        };
        let send = |local| Instruction::SendLocal { local: Reg(local) };
        let done = Instruction::Call {
            dst: Reg(9),
            result: CallResult::Discarded,
        };
        let parameters = (0..5).map(|name| parameter(name, false)).collect();
        let caller = body(
            5,
            parameters,
            vec![
                call(0),
                send(0),
                send(1),
                done,
                call(0),
                Instruction::NameArgument { name: NameId(11) },
                send(2),
                Instruction::NameArgument { name: NameId(10) },
                send(3),
                done,
                call(1),
                send(4),
                done,
            ],
        );
        let target = named(
            "App\\target",
            body(2, vec![parameter(10, false), parameter(11, true)], vec![]),
        );
        let mut unit = unit_of(vec![]);
        unit.names = (0..12).map(|index| format!("n{index}")).collect();
        unit.symbols = vec![symbol("app\\target", None), symbol("count", None)];
        unit.functions = vec![caller, target];
        unit.hoisted = vec![FunctionId(1)];
        unit
    }

    #[test]
    fn a_variable_passed_to_a_call_is_a_reference_unless_the_callee_is_certain() {
        // Names are not sealed: another file could declare target() differently.
        let open = facts(calling_unit(), false);
        assert_eq!(locals(&open, 1), [REF, REF, REF, REF, REF]);

        // Sealed: target() is the one declaration, and only $y is by reference.
        let sealed = facts(calling_unit(), true);
        assert_eq!(locals(&sealed, 1), [NONE, REF, REF, NONE, REF]);
    }

    #[test]
    fn a_namespaced_call_with_two_candidates_or_a_spread_proves_nothing() {
        let mut unit = calling_unit();
        // helper() in namespace App: App\target exists, so does the global.
        unit.symbols[0] = symbol("app\\target", Some("target"));
        assert_eq!(locals(&facts(unit, true), 1), [REF, REF, REF, REF, REF]);

        // target(...$rest, ...): positions after a spread are unknown.
        let mut unit = calling_unit();
        unit.functions[0].instructions = vec![
            Instruction::InitCall {
                function: SymbolId(0),
            },
            Instruction::SendSpread { src: Reg(8) },
            Instruction::SendLocal { local: Reg(0) },
            Instruction::Call {
                dst: Reg(9),
                result: CallResult::Discarded,
            },
        ]
        .into();
        assert_eq!(locals(&facts(unit, true), 1), [REF, NONE, NONE, NONE, NONE]);

        // A namespaced name nothing declares falls back to the global function.
        let mut unit = calling_unit();
        unit.symbols[0] = symbol("other\\target", Some("app\\target"));
        assert_eq!(locals(&facts(unit, true), 1), [NONE, REF, REF, NONE, REF]);
    }

    #[test]
    fn calls_that_do_not_nest_cleanly_prove_nothing() {
        let mut unit = calling_unit();
        // A Call without its InitCall: the pairing of sends and calls is off.
        unit.functions[0].instructions = vec![
            Instruction::InitCall {
                function: SymbolId(0),
            },
            Instruction::SendLocal { local: Reg(0) },
            Instruction::Call {
                dst: Reg(9),
                result: CallResult::Discarded,
            },
            Instruction::Call {
                dst: Reg(9),
                result: CallResult::Discarded,
            },
        ]
        .into();
        assert_eq!(locals(&facts(unit, true), 1), [REF, NONE, NONE, NONE, NONE]);
    }

    #[test]
    fn a_method_call_site_and_a_path_argument_mark_their_variables() {
        // $object->method($a);  f($b['k']);  with f unknown
        let mut caller = body(
            3,
            vec![
                parameter(0, false),
                parameter(1, false),
                parameter(2, false),
            ],
            vec![
                Instruction::InitDynamicCall { callee: Reg(2) },
                Instruction::SendPath { path: PathId(0) },
                Instruction::Call {
                    dst: Reg(9),
                    result: CallResult::Discarded,
                },
            ],
        );
        caller.paths = vec![path(1, vec![Step::Index(Reg(9))])];
        caller.site_arguments = vec![
            phera_bytecode::SiteArgument::Local(Reg(0)),
            phera_bytecode::SiteArgument::Value(Reg(9)),
        ];
        let mut unit = unit_of(vec![]);
        unit.functions = vec![caller];
        assert_eq!(locals(&facts(unit, true), 1), [REF, REF, NONE]);
    }
}
