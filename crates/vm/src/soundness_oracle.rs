//! Cold-path runtime soundness oracle for closed-world AOT [`ProgramFacts`] and
//! [`phera_bytecode::facts::UnitBuildFacts`].
//!
//! When attached to a [`Vm`] via [`Vm::set_soundness_oracle`], execution routes
//! through the single-step debug interpreter path (adding zero instructions or
//! branches to `Machine::run` or `fast_tier` when disabled) and verifies every
//! recorded fact against the live PHP state:
//!
//! 1. Locals without `LocalFlags::MAYBE_REF`/`BY_NAME` never hold a `Reference`.
//! 2. Basic-block entry `block_types` match the live register value (tag and
//!    `ClassBound::Exact` / `ClassBound::Within`).
//! 3. `Return` / `ReturnChecked` / `ReturnNull` values match `FunctionBuildFacts::returns`
//!    (and pre-coercion raw values match when `FunctionFlags::RETURN_PROVEN` holds).
//! 4. `ResolvedCall` targets (whose `Assumptions` hold) match the actual runtime
//!    callee, and `CallFlags::ARGS_EXACT` arguments satisfy callee parameter types
//!    without coercion.
//! 5. `ResolvedProp` accesses (whose `Assumptions` hold) match the runtime
//!    `PropertyLayout` slot index, hook-absence, magic-method/readonly flags, and
//!    declared property `SoundType`.
//! 6. Runtime `Class::layout` matches `AotClassLayout::slots` byte-for-byte for
//!    every `ClassFlags::LAYOUT_EXACT` class encountered.

use std::collections::HashSet;
use std::sync::Arc;

use phera_bytecode::facts::{
    CallFlags, ClassBound, ClassFlags, ClassRef, FunctionFlags, FunctionRef, ProgramFacts,
    PropAccess, SoundMask, SoundType,
};
use phera_bytecode::{FetchMode, Instruction, PathBase};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::calls::call_state::CallTarget;
use crate::calls::call_view::CallView;
use crate::calls::execution_loop::{BatchExit, YIELD_AFTER_STEPS};
use crate::calls::prepared_call::PreparedCallee;
use crate::classes::{Class, MethodBody};
use crate::code_table::FunctionIndex;

/// Counts of individual soundness assertions executed by the oracle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundnessOracleStats {
    pub steps: u64,
    pub non_ref_local_checks: u64,
    pub block_type_checks: u64,
    pub return_checks: u64,
    pub call_checks: u64,
    pub args_exact_checks: u64,
    pub prop_checks: u64,
    pub class_layout_checks: u64,
}

/// Summary collected by a [`SoundnessOracle`] run.
#[derive(Debug, Clone, Default)]
pub struct SoundnessOracleReport {
    pub stats: SoundnessOracleStats,
    pub violations: Vec<String>,
}

impl SoundnessOracleReport {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

pub(crate) struct SoundnessOracle {
    pub(crate) program_facts: Arc<ProgramFacts>,
    pub(crate) report: SoundnessOracleReport,
    checked_classes: HashSet<u32>,
}

impl SoundnessOracle {
    pub(crate) fn new(program_facts: Arc<ProgramFacts>) -> Self {
        Self {
            program_facts,
            report: SoundnessOracleReport::default(),
            checked_classes: HashSet::new(),
        }
    }

    fn record_violation(&mut self, msg: String) {
        if self.report.violations.len() < 256 {
            self.report.violations.push(msg);
        }
    }
}

impl Vm {
    /// Attach a closed-world soundness oracle to verify loaded `.phb` facts on
    /// every executed instruction. Zero cost when not attached.
    pub fn set_soundness_oracle(&mut self, program_facts: Arc<ProgramFacts>) {
        self.soundness_oracle = Some(Box::new(SoundnessOracle::new(program_facts)));
    }

    /// Returns the accumulated report if a soundness oracle is attached.
    pub fn soundness_oracle_report(&self) -> Option<&SoundnessOracleReport> {
        self.soundness_oracle.as_deref().map(|o| &o.report)
    }

    /// Cold single-step batch used when `soundness_oracle` is attached without
    /// a connected debugger.
    #[cold]
    #[inline(never)]
    pub(crate) fn oracle_batch(
        &mut self,
        stop: usize,
        work: &mut usize,
    ) -> Result<BatchExit, NativeError> {
        loop {
            let frame = self
                .current()
                .ok_or_else(|| internal_error!("missing oracle frame"))?;
            let ip = frame.ip();
            let instruction = *frame
                .function()
                .instructions
                .get(ip)
                .ok_or_else(|| internal_error!("invalid IP"))?;

            self.verify_oracle_before_step(ip, instruction);

            if let Some(exit) = self.debug_instruction(instruction, stop, work)? {
                return Ok(exit);
            }
            if self.destructors_due() {
                return Ok(BatchExit::Destructors);
            }
            if self.pending_callbacks || self.generator_releases_pending() {
                return Ok(BatchExit::Handlers);
            }
            if *work >= YIELD_AFTER_STEPS {
                *work = 0;
                self.check_execution_time()?;
                return Ok(BatchExit::Yield);
            }
        }
    }

    /// Verify all applicable closed-world facts right before `instruction` runs
    /// at `ip` in the current frame.
    #[cold]
    #[inline(never)]
    pub(crate) fn verify_oracle_before_step(&mut self, ip: usize, instruction: Instruction) {
        let Some(mut oracle) = self.soundness_oracle.take() else {
            return;
        };
        oracle.report.stats.steps += 1;

        if let Some(frame) = self.current() {
            let fn_idx = frame.frame.function;
            let orig_fn = self.code.original_function(fn_idx);
            let unit = self.code.unit(orig_fn);
            if let Some(unit_facts) = unit.facts.as_deref() {
                let facts_fn_idx = self.code.local(orig_fn).map_or(0, |i| i + 1);
                if let Some(fn_facts) = unit_facts.functions.get(facts_fn_idx) {
                    let func = self.code.function(orig_fn);
                    let file = &unit.file;
                    let fn_name = &func.name;
                    let slots = frame.slots(&self.registers);

                    // 1. Check `LocalFlags::is_proven_non_ref()` on every local.
                    for (local_idx, &flags) in fn_facts.locals.iter().enumerate() {
                        if flags.is_proven_non_ref() {
                            oracle.report.stats.non_ref_local_checks += 1;
                            if let Some(Some(Value::Reference(_))) = slots.get(local_idx) {
                                let var_name = func
                                    .locals
                                    .get(local_idx)
                                    .and_then(|nid| unit.names.get(nid.index()))
                                    .map(String::as_str)
                                    .unwrap_or("?");
                                oracle.record_violation(format!(
                                    "{file}::{fn_name}@{ip}: local ${var_name} (r{local_idx}) \
                                     marked non-ref holds a Reference"
                                ));
                            }
                        }
                    }

                    // 2. Check basic-block entry `block_types`.
                    if let Ok(block_idx) = fn_facts.block_starts.binary_search(&(ip as u32)) {
                        let b = block_idx as u16;
                        let start = fn_facts.block_types.partition_point(|&(bi, _, _)| bi < b);
                        for &(bi, reg, expected) in &fn_facts.block_types[start..] {
                            if bi != b {
                                break;
                            }
                            oracle.report.stats.block_type_checks += 1;
                            match slots.get(reg.index()).and_then(Option::as_ref) {
                                None => {
                                    if !expected.mask.intersects(SoundMask::UNDEF) {
                                        oracle.record_violation(format!(
                                            "{file}::{fn_name}@{ip} (block {b}): r{} is UNDEF, \
                                             expected {:?}",
                                            reg.0, expected
                                        ));
                                    }
                                }
                                Some(val) => {
                                    let derefed = val.deref();
                                    if !value_matches_sound_type(
                                        &derefed,
                                        expected,
                                        &oracle.program_facts,
                                    ) {
                                        oracle.record_violation(format!(
                                            "{file}::{fn_name}@{ip} (block {b}): r{} has value \
                                             type {}, expected {:?}",
                                            reg.0,
                                            describe_value_type(&derefed),
                                            expected
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    // 3. Check returns (`Return`, `ReturnChecked`, `ReturnNull`).
                    match instruction {
                        Instruction::Return { src }
                        | Instruction::ReturnChecked { source: src } => {
                            let check_raw = fn_facts.flags.contains(FunctionFlags::RETURN_PROVEN)
                                || func.return_type.is_none();
                            if check_raw && !func.is_generator {
                                oracle.report.stats.return_checks += 1;
                                let val = slots
                                    .get(src.index())
                                    .and_then(Option::as_ref)
                                    .map(|v| v.deref().clone())
                                    .unwrap_or(Value::Null);
                                if !value_matches_sound_type(
                                    &val,
                                    fn_facts.returns,
                                    &oracle.program_facts,
                                ) {
                                    oracle.record_violation(format!(
                                        "{file}::{fn_name}@{ip}: return value type {} does not \
                                         match returns {:?}",
                                        describe_value_type(&val),
                                        fn_facts.returns
                                    ));
                                }
                                if fn_facts.flags.contains(FunctionFlags::RETURN_PROVEN)
                                    && let Some(ret_tid) = func.return_type
                                    && let Some(decl_ty) = unit.types.get(ret_tid.index())
                                    && !decl_ty.mask.contains(phera_bytecode::TypeMask::VOID)
                                    && !crate::types::accepts_as_is(&val, decl_ty)
                                {
                                    oracle.record_violation(format!(
                                        "{file}::{fn_name}@{ip}: RETURN_PROVEN set, but return \
                                         value type {} failed accepts_as_is for {}",
                                        describe_value_type(&val),
                                        decl_ty.display
                                    ));
                                }
                            }
                        }
                        Instruction::ReturnNull
                            if !func.is_generator
                                && (fn_facts.flags.contains(FunctionFlags::RETURN_PROVEN)
                                    || func.return_type.is_none()) =>
                        {
                            oracle.report.stats.return_checks += 1;
                            if !value_matches_sound_type(
                                &Value::Null,
                                fn_facts.returns,
                                &oracle.program_facts,
                            ) {
                                oracle.record_violation(format!(
                                    "{file}::{fn_name}@{ip}: ReturnNull does not match \
                                     returns {:?}",
                                    fn_facts.returns
                                ));
                            }
                        }
                        _ => {}
                    }

                    // 4. Check `ResolvedCall` at `Call`, `CallDirect`, `CallMethod`.
                    if let Ok(call_idx) =
                        fn_facts.calls.binary_search_by_key(&(ip as u32), |c| c.at)
                    {
                        let resolved = fn_facts.calls[call_idx];
                        if oracle.program_facts.audit.holds.contains(resolved.needs) {
                            self.verify_resolved_call(
                                &mut oracle,
                                file,
                                fn_name,
                                ip,
                                instruction,
                                unit,
                                func,
                                unit_facts.unit,
                                resolved,
                            );
                        }
                    }

                    // 5. Check `ResolvedProp` at path instructions.
                    if let Ok(prop_idx) =
                        fn_facts.props.binary_search_by_key(&(ip as u32), |p| p.at)
                    {
                        let resolved = fn_facts.props[prop_idx];
                        if oracle.program_facts.audit.holds.contains(resolved.needs) {
                            self.verify_resolved_prop(
                                &mut oracle,
                                file,
                                fn_name,
                                ip,
                                instruction,
                                func,
                                resolved,
                            );
                        }
                    }
                }
            }
        }

        self.soundness_oracle = Some(oracle);
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_resolved_call(
        &self,
        oracle: &mut SoundnessOracle,
        file: &str,
        fn_name: &str,
        ip: usize,
        instruction: Instruction,
        unit: &phera_bytecode::CompiledUnit,
        func: &phera_bytecode::Function,
        current_unit: phera_bytecode::facts::UnitRef,
        resolved: phera_bytecode::facts::ResolvedCall,
    ) {
        oracle.report.stats.call_checks += 1;
        let Some(frame) = self.current() else {
            return;
        };
        let slots = frame.slots(&self.registers);

        match instruction {
            Instruction::CallDirect {
                function,
                args,
                count,
                ..
            } => {
                let actual = FunctionRef {
                    unit: current_unit,
                    function: function.0 + 1,
                };
                if actual != resolved.target {
                    oracle.record_violation(format!(
                        "{file}::{fn_name}@{ip}: CallDirect target {actual:?} != resolved {:?}",
                        resolved.target
                    ));
                }
                if resolved.flags.contains(CallFlags::ARGS_EXACT) {
                    let arg_vals: Vec<Value> = (0..usize::from(count))
                        .map(|off| {
                            slots
                                .get(args.index() + off)
                                .and_then(Option::as_ref)
                                .map(|v| v.deref().clone())
                                .unwrap_or(Value::Null)
                        })
                        .collect();
                    let main = self.code.facts(frame.frame.function).unit_first;
                    let target_idx = self.code.declared(main, function.index());
                    self.verify_args_exact(oracle, file, fn_name, ip, target_idx, &arg_vals);
                }
            }
            Instruction::Call { .. } => {
                let Ok(view) = CallView::top(&self.code, &self.call_stack) else {
                    return;
                };
                let target_fn = match (view.call.callee, view.callee()) {
                    (PreparedCallee::Php(idx), _) => Some(idx),
                    (PreparedCallee::General, Some(callee)) => match &callee.target {
                        CallTarget::User(idx) => Some(*idx),
                        CallTarget::Method { method, .. } => match method.body {
                            MethodBody::User(idx) => Some(idx),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                };
                let Some(actual_idx) = target_fn else {
                    oracle.record_violation(format!(
                        "{file}::{fn_name}@{ip}: Call resolved to {:?}, but runtime callee is not PHP",
                        resolved.target
                    ));
                    return;
                };
                self.verify_fn_ref_matches(oracle, file, fn_name, ip, actual_idx, resolved.target);
                if resolved.flags.contains(CallFlags::ARGS_EXACT) {
                    let arg_slots = view.slots();
                    let arg_vals: Vec<Value> = (0..view.call.sent as usize)
                        .map(|i| {
                            self.registers
                                .window(arg_slots.slot(i), 1)
                                .first()
                                .and_then(Option::as_ref)
                                .map(|v| v.deref().clone())
                                .unwrap_or(Value::Null)
                        })
                        .collect();
                    self.verify_args_exact(oracle, file, fn_name, ip, actual_idx, &arg_vals);
                }
            }
            Instruction::CallMethod { site, .. } => {
                let Some(msite) = func.method_sites.get(site.index()) else {
                    return;
                };
                let Some(Value::Object(obj)) = slots
                    .get(msite.receiver.index())
                    .and_then(Option::as_ref)
                    .map(|v| v.deref().clone())
                else {
                    oracle.record_violation(format!(
                        "{file}::{fn_name}@{ip}: CallMethod resolved to {:?}, but receiver is not an object",
                        resolved.target
                    ));
                    return;
                };
                let Some(class) = self.classes.for_object(&obj) else {
                    return;
                };
                let Some(method_name) = unit.names.get(msite.method.index()) else {
                    return;
                };
                let key = method_name.to_ascii_lowercase();
                // Account for PHP private method lexical dispatch.
                let runtime_method = frame
                    .method()
                    .and_then(|ctx| ctx.scope.methods.get(&key).cloned())
                    .filter(|m| {
                        m.visibility == phera_bytecode::Visibility::Private
                            && class.is_a(&m.declaring)
                    })
                    .or_else(|| class.methods.get(&key).cloned());
                let Some(MethodBody::User(actual_idx)) = runtime_method.as_ref().map(|m| &m.body)
                else {
                    oracle.record_violation(format!(
                        "{file}::{fn_name}@{ip}: CallMethod resolved to {:?}, but {}::{method_name} is not a user method",
                        resolved.target, class.name
                    ));
                    return;
                };
                self.verify_fn_ref_matches(oracle, file, fn_name, ip, *actual_idx, resolved.target);
                if resolved.flags.contains(CallFlags::ARGS_EXACT) {
                    let arg_vals: Vec<Value> = func.site_arguments[msite.argument_range()]
                        .iter()
                        .map(|a| {
                            slots
                                .get(a.register().index())
                                .and_then(Option::as_ref)
                                .map(|v| v.deref().clone())
                                .unwrap_or(Value::Null)
                        })
                        .collect();
                    self.verify_args_exact(oracle, file, fn_name, ip, *actual_idx, &arg_vals);
                }
            }
            _ => {}
        }
    }

    fn verify_fn_ref_matches(
        &self,
        oracle: &mut SoundnessOracle,
        file: &str,
        fn_name: &str,
        ip: usize,
        actual_idx: FunctionIndex,
        expected: FunctionRef,
    ) {
        let orig = self.code.original_function(actual_idx);
        let target_unit = self.code.unit(orig);
        let actual_unit_ref = target_unit.facts.as_ref().map(|f| f.unit);
        let actual_fn_num = self.code.local(orig).map_or(0u32, |i| (i + 1) as u32);
        if actual_unit_ref != Some(expected.unit) || actual_fn_num != expected.function {
            oracle.record_violation(format!(
                "{file}::{fn_name}@{ip}: resolved call {:?}, actual ({actual_unit_ref:?}, {actual_fn_num}, {})",
                expected,
                self.code.function(orig).name
            ));
        }
    }

    fn verify_args_exact(
        &self,
        oracle: &mut SoundnessOracle,
        file: &str,
        fn_name: &str,
        ip: usize,
        target_idx: FunctionIndex,
        args: &[Value],
    ) {
        oracle.report.stats.args_exact_checks += 1;
        let orig = self.code.original_function(target_idx);
        let callee_unit = self.code.unit(orig);
        let callee_fn = self.code.function(orig);
        for (i, (param, val)) in callee_fn.parameters.iter().zip(args.iter()).enumerate() {
            if let Some(tid) = param.ty
                && let Some(decl_ty) = callee_unit.types.get(tid.index())
                && !crate::types::accepts_as_is(val, decl_ty)
            {
                oracle.record_violation(format!(
                    "{file}::{fn_name}@{ip}: ARGS_EXACT set for {}, but arg #{i} ({}) failed accepts_as_is for {}",
                    callee_fn.name,
                    describe_value_type(val),
                    decl_ty.display
                ));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_resolved_prop(
        &self,
        oracle: &mut SoundnessOracle,
        file: &str,
        fn_name: &str,
        ip: usize,
        instruction: Instruction,
        func: &phera_bytecode::Function,
        resolved: phera_bytecode::facts::ResolvedProp,
    ) {
        oracle.report.stats.prop_checks += 1;
        let Some(frame) = self.current() else {
            return;
        };
        let path_id = match instruction {
            Instruction::Fetch { path, .. }
            | Instruction::AssignPath { path, .. }
            | Instruction::AssignPathFromLocal { path, .. }
            | Instruction::AssignOpPath { path, .. }
            | Instruction::IncDecPath { path, .. } => path,
            _ => return,
        };
        let Some(path) = func.paths.get(path_id.index()) else {
            return;
        };
        let obj: Option<std::rc::Rc<Object>> = match path.base {
            PathBase::This { .. } => frame.this().cloned(),
            PathBase::Register(r) => match frame
                .slots(&self.registers)
                .get(r.index())
                .and_then(Option::as_ref)
                .map(|v| v.deref().clone())
            {
                Some(Value::Object(obj)) => Some(obj),
                _ => None,
            },
            _ => None,
        };
        let Some(obj) = obj else {
            oracle.record_violation(format!(
                "{file}::{fn_name}@{ip}: ResolvedProp {resolved:?} reached with non-object base"
            ));
            return;
        };
        let Some(class) = self.classes.for_object(&obj) else {
            return;
        };

        // Verify the runtime class's full PropertyLayout against `AotClassLayout::slots`.
        self.verify_class_layout(oracle, &class);

        let Some(expected_class) = oracle.program_facts.classes.get(resolved.class.0 as usize)
        else {
            return;
        };
        let Some(expected_slot) = expected_class.slots.get(usize::from(resolved.slot)) else {
            oracle.record_violation(format!(
                "{file}::{fn_name}@{ip}: ResolvedProp slot {} out of bounds for {}",
                resolved.slot, expected_class.name
            ));
            return;
        };
        let expected_key = ArrayKey::interned_str(&expected_slot.key);
        let actual_slot = class.layout.as_ref().and_then(|l| l.slot(&expected_key));
        if actual_slot != Some(usize::from(resolved.slot)) {
            oracle.record_violation(format!(
                "{file}::{fn_name}@{ip}: ResolvedProp slot {} for {}::{:?}, runtime {} has slot {:?}",
                resolved.slot,
                expected_class.name,
                String::from_utf8_lossy(&expected_slot.key),
                class.name,
                actual_slot
            ));
            return;
        }

        // Verify hooks/magic/readonly invariants on the runtime class.
        if let Some(prop_def) = class.defaults.iter().find(|d| d.key == expected_key) {
            if prop_def.hooks.as_ref().is_some_and(|h| h.has_hooks()) {
                oracle.record_violation(format!(
                    "{file}::{fn_name}@{ip}: ResolvedProp points to hooked property {}::{}",
                    class.name, prop_def.name
                ));
            }
            if resolved.flags.contains(PropAccess::NON_READONLY) && prop_def.is_readonly {
                oracle.record_violation(format!(
                    "{file}::{fn_name}@{ip}: ResolvedProp marked NON_READONLY on readonly {}::{}",
                    class.name, prop_def.name
                ));
            }
        }
        if resolved.flags.contains(PropAccess::READ_EXACT)
            && class.magic.as_ref().is_some_and(|m| m.get.is_some())
        {
            oracle.record_violation(format!(
                "{file}::{fn_name}@{ip}: ResolvedProp marked READ_EXACT on {} with __get",
                class.name
            ));
        }
        if resolved.flags.contains(PropAccess::WRITE_EXACT)
            && class.magic.as_ref().is_some_and(|m| m.set.is_some())
        {
            oracle.record_violation(format!(
                "{file}::{fn_name}@{ip}: ResolvedProp marked WRITE_EXACT on {} with __set",
                class.name
            ));
        }

        // On `Fetch { mode: Read }`, if the slot is currently initialized, check its value matches `resolved.ty`.
        if let Instruction::Fetch {
            mode: FetchMode::Read,
            ..
        } = instruction
            && let Some(Some(val)) =
                obj.with_properties(|props| props.slot(usize::from(resolved.slot)).cloned())
        {
            let derefed = val.deref();
            if !value_matches_sound_type(&derefed, resolved.ty, &oracle.program_facts) {
                oracle.record_violation(format!(
                    "{file}::{fn_name}@{ip}: ResolvedProp slot {} holds {}, expected {:?}",
                    resolved.slot,
                    describe_value_type(&derefed),
                    resolved.ty
                ));
            }
        }
    }

    fn verify_class_layout(&self, oracle: &mut SoundnessOracle, class: &Class) {
        let lower = class.name.to_ascii_lowercase();
        let Ok(idx) = oracle
            .program_facts
            .classes
            .binary_search_by(|c| (*c.name).cmp(lower.as_str()))
        else {
            return;
        };
        if !oracle.checked_classes.insert(idx as u32) {
            return;
        }
        let aot = &oracle.program_facts.classes[idx];
        if !aot.flags.contains(ClassFlags::LAYOUT_EXACT) {
            return;
        }
        oracle.report.stats.class_layout_checks += 1;
        let runtime_keys: Vec<&[u8]> = class
            .layout
            .as_ref()
            .map(|l| {
                l.keys()
                    .iter()
                    .map(|k| match k {
                        ArrayKey::Interned(s) => s.as_bytes(),
                        ArrayKey::String(s) => s.as_bytes(),
                        ArrayKey::Int(_) => b"<int>",
                    })
                    .collect()
            })
            .unwrap_or_default();
        let aot_keys: Vec<&[u8]> = aot.slots.iter().map(|s| &*s.key).collect();
        if runtime_keys != aot_keys {
            oracle.record_violation(format!(
                "class {} LAYOUT_EXACT mismatch: AOT slots {:?} != runtime slots {:?}",
                class.name,
                aot_keys
                    .iter()
                    .map(|k| String::from_utf8_lossy(k))
                    .collect::<Vec<_>>(),
                runtime_keys
                    .iter()
                    .map(|k| String::from_utf8_lossy(k))
                    .collect::<Vec<_>>(),
            ));
        }
    }
}

fn value_matches_sound_type(value: &Value, expected: SoundType, facts: &ProgramFacts) -> bool {
    if expected.is_unknown() {
        return true;
    }
    let bit = match value {
        Value::Null => SoundMask::NULL,
        Value::Bool(false) => SoundMask::FALSE,
        Value::Bool(true) => SoundMask::TRUE,
        Value::Int(_) => SoundMask::INT,
        Value::Float(_) => SoundMask::FLOAT,
        Value::String(_) | Value::InternedStr(_) => SoundMask::STRING,
        Value::Array(_) => SoundMask::ARRAY,
        Value::Resource(_) => SoundMask::RESOURCE,
        Value::Object(obj) => {
            if !expected.mask.intersects(SoundMask::OBJECT) {
                return false;
            }
            return match expected.class {
                ClassBound::Any => true,
                ClassBound::Exact(ClassRef(idx)) => facts
                    .classes
                    .get(idx as usize)
                    .is_some_and(|c| obj.class.name().eq_ignore_ascii_case(&c.name)),
                ClassBound::Within(ClassRef(idx)) => facts
                    .classes
                    .get(idx as usize)
                    .is_some_and(|c| obj.class.is_instance_of(&c.name)),
            };
        }
        Value::Reference(_) => return false,
    };
    expected.mask.intersects(bit)
}

fn describe_value_type(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        Value::Int(_) => "int".to_owned(),
        Value::Float(_) => "float".to_owned(),
        Value::String(_) | Value::InternedStr(_) => "string".to_owned(),
        Value::Array(_) => "array".to_owned(),
        Value::Resource(_) => "resource".to_owned(),
        Value::Object(obj) => format!("object({})", obj.class.name()),
        Value::Reference(_) => "reference".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use phera_bytecode::Reg;
    use phera_bytecode::facts::{
        FunctionBuildFacts, FunctionFlags, LocalFlags, ProgramFacts, SoundType, UnitBuildFacts,
        UnitRef,
    };
    use phera_extension_api::Registry;

    use crate::Vm;

    #[tokio::test(flavor = "current_thread")]
    async fn oracle_catches_unsound_block_type_and_non_ref_claims() {
        let mut unit = phera_compiler::compile(
            "<?php function f($x) { $y = &$x; return $y; } echo f('hi');",
            "oracle_neg.php",
        )
        .expect("valid php");
        // Deliberately inject unsound facts for `f`: claim `$x` (r0) is `INT` at block 0
        // and that `$x`/`$y` are proven non-ref (`LocalFlags::NONE`).
        let mut fn_facts = vec![FunctionBuildFacts::default(); 1 + unit.functions.len()];
        fn_facts[1] = FunctionBuildFacts {
            flags: FunctionFlags::NONE,
            locals: vec![LocalFlags::NONE; unit.functions[0].locals.len()].into_boxed_slice(),
            block_starts: vec![0].into_boxed_slice(),
            block_types: vec![(0, Reg(0), SoundType::INT)].into_boxed_slice(),
            calls: Box::default(),
            props: Box::default(),
            returns: SoundType::INT,
        };
        unit.facts = Some(Arc::new(UnitBuildFacts {
            unit: UnitRef(0),
            classes: Box::default(),
            functions: fn_facts.into_boxed_slice(),
        }));

        let mut vm = Vm::new(Arc::new(Registry::new()));
        vm.set_soundness_oracle(Arc::new(ProgramFacts::default()));
        assert_eq!(vm.run(Arc::new(unit)).await, 0);
        assert_eq!(vm.output, b"hi");

        let report = vm.soundness_oracle_report().expect("oracle report");
        assert!(
            !report.is_clean(),
            "oracle must detect injected unsound facts"
        );
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.contains("expected SoundType") && v.contains("string")),
            "missing block_type violation: {:?}",
            report.violations
        );
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.contains("marked non-ref holds a Reference")),
            "missing non-ref violation: {:?}",
            report.violations
        );
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.contains("return value type string does not match returns")),
            "missing return violation: {:?}",
            report.violations
        );
    }
}
