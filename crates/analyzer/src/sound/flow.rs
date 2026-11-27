//! Sound intra-procedural bytecode dataflow analysis for `phera build`.
//!
//! Computes basic blocks (`block_starts`), strictly runtime-enforced register
//! types at block entry (`block_types`), sound return types (`returns`),
//! `FunctionFlags::RETURN_PROVEN`, `FunctionFlags::FAST_CANDIDATE`, and
//! resolved call / property sites (`calls`, `props`).
//!
//! Every transfer function is a strict over-approximation of PHP 8.5 runtime
//! semantics. Any local marked `LocalFlags::MAYBE_REF` or `LocalFlags::BY_NAME`
//! (or inside a `dynamic_scope` function) is conservatively widened to
//! `SoundType::UNKNOWN`, and cross-unit / closed-world facts are only fed into
//! register flow when `audit.holds.contains(needs)`.

use std::collections::VecDeque;

use phera_bytecode::facts::{
    Assumptions, ClassBound, ClassRef, FunctionBuildFacts, FunctionFlags, LocalFlags, ResolvedCall,
    ResolvedProp, SoundMask, SoundType, UnitRef,
};
use phera_bytecode::{
    BinaryOp, BuiltinFrame, BuiltinUnary, CastKind, ClassBinding, CompiledUnit, Constant,
    FetchMode, Function, IncDec, Instruction, NameId, PathBase, Reg, Step, SymbolId, Target,
    UnaryOp,
};

use super::control::{branch_target, jump_entries};
use super::resolve::ResolveContext;
use super::types::{
    MIXED_MASK, class_sound_type, is_exact_subtype, join_sound_types, sound_type_of_declared,
};

const MAX_FAST_REGS: usize = 255;
const MAX_FAST_OPS: usize = 4096;

/// `SoundType` representing an unassigned register/local (`UNDEF`).
const UNDEF_TYPE: SoundType = SoundType {
    mask: SoundMask::UNDEF,
    class: ClassBound::Any,
};

/// `SoundType` representing any defined PHP value (`mixed`, never `UNDEF`).
const DEFINED_ANY: SoundType = SoundType {
    mask: MIXED_MASK,
    class: ClassBound::Any,
};

/// Result of analyzing one function body (used to populate `FunctionBuildFacts`).
pub(crate) struct FlowOutput {
    pub(crate) block_starts: Box<[u32]>,
    pub(crate) block_types: Box<[(u16, Reg, SoundType)]>,
    pub(crate) calls: Box<[ResolvedCall]>,
    pub(crate) props: Box<[ResolvedProp]>,
    pub(crate) returns: SoundType,
    pub(crate) return_proven: bool,
    pub(crate) fast_candidate: bool,
}

#[derive(Clone, Debug)]
enum PendingCall {
    New {
        class: Option<ClassRef>,
        exact: bool,
        needs: Assumptions,
    },
    NamedFn {
        symbol: SymbolId,
        args: Option<Vec<SoundType>>,
    },
    Method {
        receiver_ty: SoundType,
        method: NameId,
        args: Option<Vec<SoundType>>,
    },
    Static {
        class: SymbolId,
        method: SymbolId,
        binding: ClassBinding,
        args: Option<Vec<SoundType>>,
    },
    Unknown,
}

impl PendingCall {
    fn push_arg(&mut self, ty: SoundType) {
        match self {
            Self::NamedFn { args, .. } | Self::Method { args, .. } | Self::Static { args, .. } => {
                if let Some(list) = args {
                    list.push(ty);
                }
            }
            Self::New { .. } | Self::Unknown => {}
        }
    }

    fn invalidate_args(&mut self) {
        match self {
            Self::NamedFn { args, .. } | Self::Method { args, .. } | Self::Static { args, .. } => {
                *args = None;
            }
            Self::New { .. } | Self::Unknown => {}
        }
    }
}

/// Analyzes one function body and produces basic blocks, sound entry types,
/// resolved calls/properties, return summary, and `FAST_CANDIDATE`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze_function(
    ctx: &ResolveContext<'_>,
    unit_ref: UnitRef,
    unit: &CompiledUnit,
    function: &Function,
    local_flags: &[LocalFlags],
    self_local_fn_id: Option<u32>,
    caller_class: Option<ClassRef>,
    emit_sites: bool,
) -> FlowOutput {
    let fast_candidate = is_fast_candidate(function, self_local_fn_id);
    let ins_len = function.instructions.len();
    let n_regs = function.registers as usize;
    if ins_len == 0 {
        let returns = if function.is_generator {
            generator_sound_type(ctx)
        } else {
            SoundType::NULL
        };
        return FlowOutput {
            block_starts: Box::default(),
            block_types: Box::default(),
            calls: Box::default(),
            props: Box::default(),
            returns,
            return_proven: false,
            fast_candidate,
        };
    }

    let (block_starts, ip_to_block) = build_basic_blocks(function);
    let n_blocks = block_starts.len();

    // Precompute all `FinallyCall` resume targets (`ip + 1`).
    let finally_resumes: Vec<usize> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(ip, inst)| {
            matches!(inst, Instruction::FinallyCall { .. })
                .then_some(ip + 1)
                .filter(|&next| next < ins_len)
        })
        .collect();

    // Per-block entry state (`None` = unreachable so far).
    let mut entry_states: Vec<Option<Vec<SoundType>>> = vec![None; n_blocks];
    let init = initial_entry_state(ctx, unit, function, local_flags, caller_class);
    entry_states[0] = Some(init);

    let mut worklist = VecDeque::new();
    let mut in_queue = vec![false; n_blocks];
    worklist.push_back(0usize);
    in_queue[0] = true;

    let mut returned_type = SoundType::NEVER;
    let mut has_return_path = false;
    let mut may_fall_off_end = false;

    while let Some(bi) = worklist.pop_front() {
        in_queue[bi] = false;
        let Some(mut state) = entry_states[bi].clone() else {
            continue;
        };
        let start = block_starts[bi] as usize;
        let end = if bi + 1 < n_blocks {
            block_starts[bi + 1] as usize
        } else {
            ins_len
        };

        let mut call_stack: Vec<PendingCall> = Vec::new();

        // Propagate exceptional edges from any handler covering instructions in this block.
        let propagate_exceptional = |st: &[SoundType],
                                     ip: usize,
                                     entry_states: &mut [Option<Vec<SoundType>>],
                                     worklist: &mut VecDeque<usize>,
                                     in_queue: &mut [bool]| {
            for handler in &function.handlers {
                if ip >= handler.start.index() && ip < handler.end.index() {
                    if let Some(catch) = handler.catch
                        && let Some(&target_b) = ip_to_block.get(catch.index())
                    {
                        let mut ex_state = st.to_vec();
                        for &cleanup in &handler.cleanup {
                            if let Some(slot) = ex_state.get_mut(cleanup.index()) {
                                *slot = UNDEF_TYPE;
                            }
                        }
                        if let Some(slot) = ex_state.get_mut(handler.exception.index()) {
                            *slot = SoundType {
                                mask: SoundMask::OBJECT,
                                class: ClassBound::Any,
                            };
                        }
                        merge_into_block(
                            target_b,
                            &ex_state,
                            ctx,
                            entry_states,
                            worklist,
                            in_queue,
                        );
                    }
                    if let Some(finally) = handler.finally
                        && let Some(&target_b) = ip_to_block.get(finally.index())
                    {
                        let mut fin_state = st.to_vec();
                        for &cleanup in &handler.cleanup {
                            if let Some(slot) = fin_state.get_mut(cleanup.index()) {
                                *slot = UNDEF_TYPE;
                            }
                        }
                        if let Some(slot) = fin_state.get_mut(handler.exception.index()) {
                            *slot = SoundType::UNKNOWN;
                        }
                        merge_into_block(
                            target_b,
                            &fin_state,
                            ctx,
                            entry_states,
                            worklist,
                            in_queue,
                        );
                    }
                }
            }
        };

        let mut fell_through = true;
        for ip in start..end {
            if !function.handlers.is_empty() {
                propagate_exceptional(&state, ip, &mut entry_states, &mut worklist, &mut in_queue);
            }
            let inst = function.instructions[ip];
            let step = step_instruction(
                ctx,
                unit_ref,
                unit,
                function,
                local_flags,
                caller_class,
                ip,
                inst,
                &mut state,
                &mut call_stack,
                None,
                None,
            );
            if !function.handlers.is_empty() {
                propagate_exceptional(&state, ip, &mut entry_states, &mut worklist, &mut in_queue);
            }

            if let Some(ret_ty) = step.returned {
                has_return_path = true;
                returned_type = join_sound_types(returned_type, ret_ty, ctx.hierarchy);
            }

            if let Some((target, narrowed)) = step.branch
                && let Some(&target_b) = ip_to_block.get(target.index())
            {
                let st = narrowed.as_deref().unwrap_or(&state);
                merge_into_block(
                    target_b,
                    st,
                    ctx,
                    &mut entry_states,
                    &mut worklist,
                    &mut in_queue,
                );
            }
            if let Some(extra_target) = step.extra_target
                && let Some(&target_b) = ip_to_block.get(extra_target.index())
            {
                merge_into_block(
                    target_b,
                    &state,
                    ctx,
                    &mut entry_states,
                    &mut worklist,
                    &mut in_queue,
                );
            }
            if step.is_finally_return {
                for &resume_ip in &finally_resumes {
                    if let Some(&target_b) = ip_to_block.get(resume_ip) {
                        merge_into_block(
                            target_b,
                            &state,
                            ctx,
                            &mut entry_states,
                            &mut worklist,
                            &mut in_queue,
                        );
                    }
                }
            }
            if let Some(fallthrough_narrow) = step.fallthrough_narrow {
                state = fallthrough_narrow;
            }
            if !step.falls_through {
                fell_through = false;
                break;
            }
        }

        if fell_through {
            if bi + 1 < n_blocks {
                merge_into_block(
                    bi + 1,
                    &state,
                    ctx,
                    &mut entry_states,
                    &mut worklist,
                    &mut in_queue,
                );
            } else {
                may_fall_off_end = true;
            }
        }
    }

    if may_fall_off_end {
        has_return_path = true;
        returned_type = join_sound_types(returned_type, SoundType::NULL, ctx.hierarchy);
    }

    // Final extraction pass: collect `calls` and `props` from converged block
    // entry states (only when `emit_sites` is true).
    let mut calls = Vec::new();
    let mut props = Vec::new();
    if emit_sites {
        for bi in 0..n_blocks {
            let Some(mut state) = entry_states[bi].clone() else {
                continue;
            };
            let start = block_starts[bi] as usize;
            let end = if bi + 1 < n_blocks {
                block_starts[bi + 1] as usize
            } else {
                ins_len
            };
            let mut call_stack: Vec<PendingCall> = Vec::new();
            for ip in start..end {
                let inst = function.instructions[ip];
                let step = step_instruction(
                    ctx,
                    unit_ref,
                    unit,
                    function,
                    local_flags,
                    caller_class,
                    ip,
                    inst,
                    &mut state,
                    &mut call_stack,
                    Some(&mut calls),
                    Some(&mut props),
                );
                if let Some(fallthrough_narrow) = step.fallthrough_narrow {
                    state = fallthrough_narrow;
                }
                if !step.falls_through {
                    break;
                }
            }
        }
        calls.sort_by_key(|c| c.at);
        calls.dedup_by_key(|c| c.at);
        props.sort_by_key(|p| p.at);
        props.dedup_by_key(|p| p.at);
    }

    // Extract compact `block_types` (non-UNKNOWN, non-UNDEF, valid u16 block index).
    let mut block_types: Vec<(u16, Reg, SoundType)> = Vec::new();
    if n_blocks <= usize::from(u16::MAX) {
        for (bi, state) in entry_states.iter().enumerate() {
            let Some(regs) = state else {
                continue;
            };
            for (r, &ty) in regs.iter().enumerate().take(n_regs) {
                if ty.is_unknown()
                    || ty.mask == SoundMask::NONE
                    || ty.mask.intersects(SoundMask::UNDEF)
                {
                    continue;
                }
                // Only record non-trivial facts (skip plain `mixed` with no class bound
                // unless it actually excludes `UNDEF` on a local that would otherwise be unknown).
                if ty.mask == MIXED_MASK && matches!(ty.class, ClassBound::Any) {
                    continue;
                }
                block_types.push((bi as u16, Reg(r as u16), ty));
            }
        }
    }

    // Compute final `returns` and `RETURN_PROVEN`.
    let (returns, return_proven) = if function.is_generator {
        (generator_sound_type(ctx), false)
    } else if !has_return_path {
        (SoundType::NEVER, false)
    } else if let Some(ret_tid) = function.return_type {
        let exact = !function.returns_reference
            && is_exact_subtype(returned_type, unit, ret_tid, ctx.hierarchy, caller_class);
        if exact {
            (returned_type, true)
        } else {
            // PHP coerces or throws on return, so any returned value satisfies
            // the declared return type.
            let declared = sound_type_of_declared(unit, ret_tid, ctx.hierarchy, caller_class);
            (declared, false)
        }
    } else {
        (returned_type, false)
    };

    FlowOutput {
        block_starts: block_starts.into_boxed_slice(),
        block_types: block_types.into_boxed_slice(),
        calls: calls.into_boxed_slice(),
        props: props.into_boxed_slice(),
        returns,
        return_proven,
        fast_candidate,
    }
}

fn build_basic_blocks(function: &Function) -> (Vec<u32>, Vec<usize>) {
    let ins_len = function.instructions.len();
    let mut is_start = jump_entries(function);
    is_start[0] = true;
    for (ip, inst) in function.instructions.iter().enumerate() {
        let splits_after = branch_target(inst).is_some()
            || matches!(
                inst,
                Instruction::Return { .. }
                    | Instruction::ReturnNull
                    | Instruction::ReturnReference { .. }
                    | Instruction::ReturnChecked { .. }
                    | Instruction::ReturnMissing
                    | Instruction::GeneratorReturn { .. }
                    | Instruction::Throw { .. }
                    | Instruction::Exit { .. }
                    | Instruction::Raise { .. }
                    | Instruction::FinallyReturn { .. }
                    | Instruction::CallMethod { .. }
            );
        if splits_after && ip + 1 < ins_len {
            is_start[ip + 1] = true;
        }
    }

    let mut block_starts = Vec::new();
    let mut ip_to_block = vec![0usize; ins_len];
    for (ip, &start) in is_start.iter().enumerate() {
        if start {
            block_starts.push(ip as u32);
        }
        ip_to_block[ip] = block_starts.len() - 1;
    }
    (block_starts, ip_to_block)
}

fn initial_entry_state(
    ctx: &ResolveContext<'_>,
    unit: &CompiledUnit,
    function: &Function,
    local_flags: &[LocalFlags],
    caller_class: Option<ClassRef>,
) -> Vec<SoundType> {
    let n_regs = function.registers as usize;
    let n_locals = function.locals.len();
    let mut state = vec![UNDEF_TYPE; n_regs];

    if function.dynamic_scope {
        for slot in state.iter_mut().take(n_locals) {
            *slot = SoundType::UNKNOWN;
        }
        return state;
    }

    for (i, param) in function.parameters.iter().enumerate() {
        if i >= n_regs {
            break;
        }
        let flags = local_flags.get(i).copied().unwrap_or(LocalFlags::MAYBE_REF);
        if !flags.is_proven_non_ref() || param.by_reference {
            state[i] = SoundType::UNKNOWN;
            continue;
        }
        let param_ty = if param.variadic {
            SoundType::ARRAY
        } else if let Some(tid) = param.ty {
            sound_type_of_declared(unit, tid, ctx.hierarchy, caller_class)
        } else {
            DEFINED_ANY
        };
        if param.has_default {
            // Before `ReceiveDefault` executes, omitted optional parameters are empty.
            state[i] = param_ty.union(UNDEF_TYPE);
        } else {
            state[i] = param_ty;
        }
    }

    // Lexical captures follow parameters and are initialized at closure entry.
    let capture_start = function.parameters.len();
    let capture_end = (capture_start + usize::from(function.captures)).min(n_regs);
    for slot in &mut state[capture_start..capture_end] {
        *slot = SoundType::UNKNOWN;
    }

    // Named locals that may be refs/by-name start as UNKNOWN so they never get
    // narrowed prematurely.
    for (i, slot) in state
        .iter_mut()
        .enumerate()
        .take(n_locals.min(n_regs))
        .skip(capture_end)
    {
        let flags = local_flags.get(i).copied().unwrap_or(LocalFlags::NONE);
        if !flags.is_proven_non_ref() {
            *slot = SoundType::UNKNOWN;
        }
    }

    state
}

fn merge_into_block(
    target_block: usize,
    incoming: &[SoundType],
    ctx: &ResolveContext<'_>,
    entry_states: &mut [Option<Vec<SoundType>>],
    worklist: &mut VecDeque<usize>,
    in_queue: &mut [bool],
) {
    let changed = match &mut entry_states[target_block] {
        None => {
            entry_states[target_block] = Some(incoming.to_vec());
            true
        }
        Some(existing) => {
            let mut changed = false;
            for (dst, &src) in existing.iter_mut().zip(incoming.iter()) {
                let joined = join_sound_types(*dst, src, ctx.hierarchy);
                if joined != *dst {
                    *dst = joined;
                    changed = true;
                }
            }
            changed
        }
    };
    if changed && !in_queue[target_block] {
        in_queue[target_block] = true;
        worklist.push_back(target_block);
    }
}

struct StepOutcome {
    falls_through: bool,
    branch: Option<(Target, Option<Vec<SoundType>>)>,
    extra_target: Option<Target>,
    fallthrough_narrow: Option<Vec<SoundType>>,
    returned: Option<SoundType>,
    is_finally_return: bool,
}

impl Default for StepOutcome {
    fn default() -> Self {
        Self {
            falls_through: true,
            branch: None,
            extra_target: None,
            fallthrough_narrow: None,
            returned: None,
            is_finally_return: false,
        }
    }
}

#[inline]
fn can_track_reg(reg: Reg, function: &Function, local_flags: &[LocalFlags]) -> bool {
    let idx = reg.index();
    if idx >= function.locals.len() {
        return true;
    }
    if function.dynamic_scope {
        return false;
    }
    local_flags
        .get(idx)
        .is_some_and(|flags| flags.is_proven_non_ref())
}

#[inline]
fn set_reg(
    state: &mut [SoundType],
    reg: Reg,
    ty: SoundType,
    function: &Function,
    local_flags: &[LocalFlags],
) {
    let Some(slot) = state.get_mut(reg.index()) else {
        return;
    };
    if can_track_reg(reg, function, local_flags) {
        *slot = ty;
    } else {
        *slot = SoundType::UNKNOWN;
    }
}

/// Reads the dereferenced PHP value of `reg` (an `UNDEF` local yields `NULL`
/// after warning).
#[inline]
fn read_val(state: &[SoundType], reg: Reg) -> SoundType {
    let ty = state
        .get(reg.index())
        .copied()
        .unwrap_or(SoundType::UNKNOWN);
    if ty.mask.intersects(SoundMask::UNDEF) {
        SoundType {
            mask: SoundMask((ty.mask.0 & !SoundMask::UNDEF.0) | SoundMask::NULL.0),
            class: ty.class,
        }
    } else {
        ty
    }
}

fn widen_dynamic_locals(state: &mut [SoundType], function: &Function) {
    if function.dynamic_scope {
        let n = function.locals.len().min(state.len());
        for slot in &mut state[..n] {
            *slot = SoundType::UNKNOWN;
        }
    }
}

fn mark_path_write_base(
    state: &mut [SoundType],
    base: PathBase,
    steps: &[Step],
    function: &Function,
    local_flags: &[LocalFlags],
) {
    if matches!(base, PathBase::Named(_) | PathBase::GlobalNamed { .. }) {
        widen_dynamic_locals(state, function);
    }
    if let PathBase::Register(base_reg) = base
        && matches!(steps.first(), Some(Step::Index(_) | Step::Append))
    {
        // Writing `$a[$k] = $v`, `$a[$k] += 1`, `$a[$k]++`, or `$a[] = $v` may
        // auto-vivify an undefined/null local into an array, or mutate a string/object.
        let cur = read_val(state, base_reg);
        if !cur
            .mask
            .intersects(SoundMask::STRING.union(SoundMask::OBJECT))
        {
            set_reg(state, base_reg, SoundType::ARRAY, function, local_flags);
        } else {
            set_reg(
                state,
                base_reg,
                cur.union(SoundType::ARRAY),
                function,
                local_flags,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn step_instruction(
    ctx: &ResolveContext<'_>,
    unit_ref: UnitRef,
    unit: &CompiledUnit,
    function: &Function,
    local_flags: &[LocalFlags],
    caller_class: Option<ClassRef>,
    ip: usize,
    inst: Instruction,
    state: &mut [SoundType],
    call_stack: &mut Vec<PendingCall>,
    mut out_calls: Option<&mut Vec<ResolvedCall>>,
    mut out_props: Option<&mut Vec<ResolvedProp>>,
) -> StepOutcome {
    let mut out = StepOutcome::default();

    match inst {
        Instruction::Nop
        | Instruction::GeneratorStart
        | Instruction::AttributeDiagnostics
        | Instruction::BeginSilence
        | Instruction::EndSilence
        | Instruction::Echo { .. }
        | Instruction::DeclareFunction { .. }
        | Instruction::DeclareClass { .. }
        | Instruction::DeclareConstant { .. }
        | Instruction::IterEnd { .. }
        | Instruction::Capture { .. }
        | Instruction::FinallyDiscard { .. } => {}

        Instruction::CheckReturn { source } => {
            if let Some(ret_tid) = function.return_type {
                let cur = read_val(state, source);
                if !is_exact_subtype(cur, unit, ret_tid, ctx.hierarchy, caller_class) {
                    let declared =
                        sound_type_of_declared(unit, ret_tid, ctx.hierarchy, caller_class);
                    set_reg(
                        state,
                        source,
                        join_sound_types(cur, declared, ctx.hierarchy),
                        function,
                        local_flags,
                    );
                }
            }
        }

        Instruction::LoadInt { dst, .. } => {
            set_reg(state, dst, SoundType::INT, function, local_flags);
        }
        Instruction::LoadBool { dst, value } => {
            let ty = SoundType {
                mask: if value {
                    SoundMask::TRUE
                } else {
                    SoundMask::FALSE
                },
                class: ClassBound::Any,
            };
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::LoadNull { dst } => {
            set_reg(state, dst, SoundType::NULL, function, local_flags);
        }
        Instruction::LoadConst { dst, constant } => {
            let ty = unit
                .constants
                .get(constant.index())
                .map_or(SoundType::UNKNOWN, constant_sound_type);
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::AssignConst { local, constant } => {
            let ty = unit
                .constants
                .get(constant.index())
                .map_or(SoundType::UNKNOWN, constant_sound_type);
            set_reg(state, local, ty, function, local_flags);
        }
        Instruction::NewArray { dst, .. } => {
            set_reg(state, dst, SoundType::ARRAY, function, local_flags);
        }
        Instruction::ArrayPush { .. }
        | Instruction::ArraySet { .. }
        | Instruction::ArraySpread { .. }
        | Instruction::ArrayPushReference { .. }
        | Instruction::ArraySetReference { .. } => {}

        Instruction::Move { dst, src } => {
            let ty = read_val(state, src);
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::Assign { local, src } => {
            let ty = read_val(state, src);
            set_reg(state, local, ty, function, local_flags);
        }
        Instruction::AssignTake { local, src } => {
            let ty = read_val(state, src);
            set_reg(state, local, ty, function, local_flags);
            if src.index() >= function.locals.len()
                && let Some(slot) = state.get_mut(src.index())
            {
                *slot = UNDEF_TYPE;
            }
        }
        Instruction::Clear { reg } => {
            if reg.index() >= function.locals.len()
                && let Some(slot) = state.get_mut(reg.index())
            {
                *slot = UNDEF_TYPE;
            }
        }
        Instruction::UnsetLocal { local } => {
            if can_track_reg(local, function, local_flags)
                && let Some(slot) = state.get_mut(local.index())
            {
                *slot = UNDEF_TYPE;
            }
        }
        Instruction::IssetLocal { dst, .. }
        | Instruction::IssetPath { dst, .. }
        | Instruction::InstanceOf { dst, .. }
        | Instruction::DynamicInstanceOf { dst, .. }
        | Instruction::ArrayKeyExists { dst, .. }
        | Instruction::InLiteralArray { dst, .. } => {
            set_reg(state, dst, SoundType::BOOL, function, local_flags);
        }

        Instruction::FetchClassName { dst, .. } => {
            set_reg(state, dst, SoundType::STRING, function, local_flags);
        }
        Instruction::FetchThis { dst } => {
            let ty = this_sound_type(ctx, caller_class);
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::ThisOrNull { dst } => {
            let ty = this_sound_type(ctx, caller_class).union(SoundType::NULL);
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::Clone { dst, src, .. } => {
            let src_ty = read_val(state, src);
            let ty = SoundType {
                mask: SoundMask::OBJECT,
                class: if src_ty.mask.intersects(SoundMask::OBJECT) {
                    src_ty.class
                } else {
                    ClassBound::Any
                },
            };
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::MakeClosure { dst, .. } => {
            set_reg(state, dst, closure_sound_type(ctx), function, local_flags);
        }
        Instruction::MakeCallable { dst } => {
            call_stack.pop();
            set_reg(state, dst, closure_sound_type(ctx), function, local_flags);
        }

        Instruction::Binary { op, dst, lhs, rhs } => {
            let ty = binary_sound_type(op, read_val(state, lhs), read_val(state, rhs));
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::BinaryInt { op, dst, lhs, .. } => {
            let ty = binary_sound_type(op, read_val(state, lhs), SoundType::INT);
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::AssignOp { op, local, src } => {
            let ty = binary_sound_type(op, read_val(state, local), read_val(state, src));
            set_reg(state, local, ty, function, local_flags);
        }
        Instruction::Unary { op, dst, src } => {
            let ty = unary_sound_type(op, read_val(state, src));
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::Cast { to, dst, src } => {
            let ty = cast_sound_type(to, read_val(state, src));
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::IncDecLocal { mode, dst, local } => {
            let before = read_val(state, local);
            let after = inc_dec_sound_type(mode, before);
            set_reg(state, local, after, function, local_flags);
            let expr_ty = match mode {
                IncDec::PreIncrement | IncDec::PreDecrement => after,
                IncDec::PostIncrement | IncDec::PostDecrement => before,
            };
            set_reg(state, dst, expr_ty, function, local_flags);
        }

        Instruction::CheckType { local, ty: tid } => {
            let declared = sound_type_of_declared(unit, tid, ctx.hierarchy, caller_class);
            set_reg(state, local, declared, function, local_flags);
        }

        Instruction::BuiltinUnary { op, dst, .. }
        | Instruction::NamespacedBuiltin { op, dst, .. } => {
            let ty = builtin_unary_sound_type(op);
            set_reg(state, dst, ty, function, local_flags);
        }
        Instruction::BuiltinFrame { op, dst } => {
            let ty = match op {
                BuiltinFrame::ArgumentCount => SoundType::INT,
                BuiltinFrame::ArgumentsFrom(_) => SoundType::ARRAY,
                BuiltinFrame::CalledClass => SoundType::STRING,
                BuiltinFrame::AssertionsDisabled => SoundType::BOOL,
            };
            set_reg(state, dst, ty, function, local_flags);
        }

        Instruction::Fetch { dst, path, mode } => {
            let mut result_ty = SoundType::UNKNOWN;
            if let Some(p) = function.paths.get(path.index())
                && let Some(base_ty) = path_base_type(ctx, p.base, state, caller_class)
                && let Some(resolved) = ctx.resolve_property_path(
                    ip as u32,
                    unit_ref,
                    unit,
                    p.base,
                    &p.steps,
                    base_ty,
                    caller_class,
                    None,
                )
            {
                if ctx.audit_holds.contains(resolved.needs) {
                    result_ty = match mode {
                        FetchMode::Read => resolved.ty,
                        FetchMode::Quiet
                        | FetchMode::Isset
                        | FetchMode::Empty
                        | FetchMode::Destructure => resolved.ty.union(SoundType::NULL),
                    };
                }
                if let Some(props) = out_props.as_mut() {
                    props.push(resolved);
                }
            }
            set_reg(state, dst, result_ty, function, local_flags);
        }
        Instruction::FetchIndex { dst, .. } => {
            set_reg(state, dst, SoundType::UNKNOWN, function, local_flags);
        }
        Instruction::AppendLocal { local, .. } => {
            // `$a[] = $x` turns null/undef/array into array (or throws on scalar/object).
            let cur = read_val(state, local);
            let ty = if cur.mask.intersects(SoundMask::OBJECT) {
                SoundType::ARRAY.union(SoundType {
                    mask: SoundMask::OBJECT,
                    class: cur.class,
                })
            } else {
                SoundType::ARRAY
            };
            set_reg(state, local, ty, function, local_flags);
        }
        Instruction::AssignPath { path, src } => {
            let rhs_ty = read_val(state, src);
            if let Some(p) = function.paths.get(path.index()) {
                if let Some(base_ty) = path_base_type(ctx, p.base, state, caller_class)
                    && let Some(resolved) = ctx.resolve_property_path(
                        ip as u32,
                        unit_ref,
                        unit,
                        p.base,
                        &p.steps,
                        base_ty,
                        caller_class,
                        Some(rhs_ty),
                    )
                    && let Some(props) = out_props.as_mut()
                {
                    props.push(resolved);
                }
                mark_path_write_base(state, p.base, &p.steps, function, local_flags);
            }
        }
        Instruction::AssignPathFromLocal { path, src, dst } => {
            let rhs_ty = read_val(state, src);
            if let Some(p) = function.paths.get(path.index()) {
                if let Some(base_ty) = path_base_type(ctx, p.base, state, caller_class)
                    && let Some(resolved) = ctx.resolve_property_path(
                        ip as u32,
                        unit_ref,
                        unit,
                        p.base,
                        &p.steps,
                        base_ty,
                        caller_class,
                        Some(rhs_ty),
                    )
                    && let Some(props) = out_props.as_mut()
                {
                    props.push(resolved);
                }
                mark_path_write_base(state, p.base, &p.steps, function, local_flags);
            }
            set_reg(state, dst, rhs_ty, function, local_flags);
        }
        Instruction::AssignOpPath { op, dst, path, src } => {
            let mut result_ty = SoundType::UNKNOWN;
            if let Some(p) = function.paths.get(path.index()) {
                if let Some(base_ty) = path_base_type(ctx, p.base, state, caller_class)
                    && let Some(resolved) = ctx.resolve_property_path(
                        ip as u32,
                        unit_ref,
                        unit,
                        p.base,
                        &p.steps,
                        base_ty,
                        caller_class,
                        None,
                    )
                {
                    if ctx.audit_holds.contains(resolved.needs) {
                        let rhs_ty = read_val(state, src);
                        let computed = binary_sound_type(op, resolved.ty, rhs_ty);
                        // If the property is typed, PHP coerces/checks on store.
                        if resolved.ty == SoundType::UNKNOWN {
                            result_ty = computed;
                        } else {
                            result_ty = resolved.ty;
                        }
                    }
                    if let Some(props) = out_props.as_mut() {
                        props.push(resolved);
                    }
                }
                mark_path_write_base(state, p.base, &p.steps, function, local_flags);
            }
            set_reg(state, dst, result_ty, function, local_flags);
        }
        Instruction::IncDecPath { mode, dst, path } => {
            let mut result_ty = SoundType::UNKNOWN;
            if let Some(p) = function.paths.get(path.index()) {
                if let Some(base_ty) = path_base_type(ctx, p.base, state, caller_class)
                    && let Some(resolved) = ctx.resolve_property_path(
                        ip as u32,
                        unit_ref,
                        unit,
                        p.base,
                        &p.steps,
                        base_ty,
                        caller_class,
                        None,
                    )
                {
                    if ctx.audit_holds.contains(resolved.needs) && resolved.ty != SoundType::UNKNOWN
                    {
                        result_ty = match mode {
                            IncDec::PostIncrement | IncDec::PostDecrement => resolved.ty,
                            IncDec::PreIncrement | IncDec::PreDecrement => {
                                inc_dec_sound_type(mode, resolved.ty)
                            }
                        };
                    }
                    if let Some(props) = out_props.as_mut() {
                        props.push(resolved);
                    }
                }
                mark_path_write_base(state, p.base, &p.steps, function, local_flags);
            }
            set_reg(state, dst, result_ty, function, local_flags);
        }
        Instruction::UnsetPath { path } => {
            if let Some(p) = function.paths.get(path.index())
                && matches!(p.base, PathBase::Named(_) | PathBase::GlobalNamed { .. })
            {
                widen_dynamic_locals(state, function);
            }
        }

        Instruction::MakeReference { dst, .. } => {
            set_reg(state, dst, SoundType::UNKNOWN, function, local_flags);
        }
        Instruction::BindReference { .. } => {
            widen_dynamic_locals(state, function);
        }
        Instruction::BindGlobal { local, .. } | Instruction::InitStatic { local, .. } => {
            set_reg(state, local, SoundType::UNKNOWN, function, local_flags);
        }
        Instruction::BindDynamicGlobal { .. } => {
            widen_dynamic_locals(state, function);
        }
        Instruction::BindStatic {
            local, initialized, ..
        } => {
            set_reg(state, local, SoundType::UNKNOWN, function, local_flags);
            out.branch = Some((initialized, None));
        }

        Instruction::FetchConstant { dst, .. }
        | Instruction::FetchClassConstant { dst, .. }
        | Instruction::FetchNamedClassConstant { dst, .. }
        | Instruction::FetchDynamicClassConstant { dst, .. }
        | Instruction::Yield { dst, .. }
        | Instruction::YieldFrom { dst, .. } => {
            set_reg(state, dst, SoundType::UNKNOWN, function, local_flags);
        }

        Instruction::Include { dst, .. } | Instruction::Eval { dst, .. } => {
            widen_dynamic_locals(state, function);
            set_reg(state, dst, SoundType::UNKNOWN, function, local_flags);
        }

        // ---- Calls ----
        Instruction::New { class } => {
            let pending = match ctx.resolve_new_class(unit, class, caller_class) {
                Some((cref, exact, needs)) => PendingCall::New {
                    class: Some(cref),
                    exact,
                    needs,
                },
                None => PendingCall::New {
                    class: None,
                    exact: false,
                    needs: Assumptions::NONE,
                },
            };
            call_stack.push(pending);
        }
        Instruction::NewDynamic { .. } => {
            call_stack.push(PendingCall::New {
                class: None,
                exact: false,
                needs: Assumptions::NONE,
            });
        }
        Instruction::InitCall { function: sym } => {
            call_stack.push(PendingCall::NamedFn {
                symbol: sym,
                args: Some(Vec::new()),
            });
        }
        Instruction::InitMethodCall { receiver, method } => {
            call_stack.push(PendingCall::Method {
                receiver_ty: read_val(state, receiver),
                method,
                args: Some(Vec::new()),
            });
        }
        Instruction::InitStaticCall {
            class,
            method,
            binding,
        } => {
            call_stack.push(PendingCall::Static {
                class,
                method,
                binding,
                args: Some(Vec::new()),
            });
        }
        Instruction::InitDynamicCall { .. }
        | Instruction::DynamicMethodCall { .. }
        | Instruction::StaticCall { .. }
        | Instruction::InitUserCallback { .. } => {
            call_stack.push(PendingCall::Unknown);
        }
        Instruction::SendValue { src, .. }
        | Instruction::SendTake { src, .. }
        | Instruction::SendLocal { local: src } => {
            let ty = read_val(state, src);
            if let Some(top) = call_stack.last_mut() {
                top.push_arg(ty);
            }
            if matches!(inst, Instruction::SendTake { .. })
                && src.index() >= function.locals.len()
                && let Some(slot) = state.get_mut(src.index())
            {
                *slot = UNDEF_TYPE;
            }
        }
        Instruction::SendConst { constant } => {
            let ty = unit
                .constants
                .get(constant.index())
                .map_or(SoundType::UNKNOWN, constant_sound_type);
            if let Some(top) = call_stack.last_mut() {
                top.push_arg(ty);
            }
        }
        Instruction::SendPath { .. } => {
            if let Some(top) = call_stack.last_mut() {
                top.push_arg(SoundType::UNKNOWN);
            }
        }
        Instruction::SendSpread { .. }
        | Instruction::NameArgument { .. }
        | Instruction::SendUserCallback { .. } => {
            if let Some(top) = call_stack.last_mut() {
                top.invalidate_args();
            }
        }
        Instruction::Call { dst, .. } => {
            widen_dynamic_locals(state, function);
            let mut result_ty = SoundType::UNKNOWN;
            if let Some(pending) = call_stack.pop() {
                match pending {
                    PendingCall::New {
                        class: Some(cref),
                        exact,
                        needs,
                    } => {
                        if ctx.audit_holds.contains(needs) {
                            result_ty = if exact {
                                SoundType::exact_class(cref)
                            } else {
                                class_sound_type(cref, ctx.hierarchy)
                            };
                        } else {
                            result_ty = SoundType {
                                mask: SoundMask::OBJECT,
                                class: ClassBound::Any,
                            };
                        }
                    }
                    PendingCall::New { class: None, .. } => {
                        result_ty = SoundType {
                            mask: SoundMask::OBJECT,
                            class: ClassBound::Any,
                        };
                    }
                    PendingCall::NamedFn { symbol, args } => {
                        if let Some(resolved) = ctx.resolve_named_call(
                            ip as u32,
                            unit_ref,
                            unit,
                            symbol,
                            args.as_deref(),
                        ) {
                            if ctx.audit_holds.contains(resolved.needs) {
                                result_ty = resolved.result;
                            }
                            if let Some(calls) = out_calls.as_mut() {
                                calls.push(resolved);
                            }
                        }
                    }
                    PendingCall::Method {
                        receiver_ty,
                        method,
                        args,
                    } => {
                        if let Some(resolved) = ctx.resolve_method_call(
                            ip as u32,
                            unit_ref,
                            unit,
                            receiver_ty,
                            method,
                            caller_class,
                            args.as_deref(),
                        ) {
                            if ctx.audit_holds.contains(resolved.needs) {
                                result_ty = resolved.result;
                            }
                            if let Some(calls) = out_calls.as_mut() {
                                calls.push(resolved);
                            }
                        }
                    }
                    PendingCall::Static {
                        class,
                        method,
                        binding,
                        args,
                    } => {
                        if let Some(resolved) = ctx.resolve_static_call(
                            ip as u32,
                            unit_ref,
                            unit,
                            class,
                            method,
                            binding,
                            caller_class,
                            args.as_deref(),
                        ) {
                            if ctx.audit_holds.contains(resolved.needs) {
                                result_ty = resolved.result;
                            }
                            if let Some(calls) = out_calls.as_mut() {
                                calls.push(resolved);
                            }
                        }
                    }
                    PendingCall::Unknown => {}
                }
            }
            set_reg(state, dst, result_ty, function, local_flags);
        }
        Instruction::CallDirect {
            function: fn_id,
            args,
            count,
            dst,
            ..
        } => {
            widen_dynamic_locals(state, function);
            let mut arg_types = Vec::with_capacity(usize::from(count));
            for offset in 0..usize::from(count) {
                arg_types.push(read_val(state, Reg(args.0 + offset as u16)));
            }
            let mut result_ty = SoundType::UNKNOWN;
            if let Some(resolved) = ctx.resolve_call_direct(ip as u32, unit_ref, fn_id, &arg_types)
            {
                if ctx.audit_holds.contains(resolved.needs) {
                    result_ty = resolved.result;
                }
                if let Some(calls) = out_calls.as_mut() {
                    calls.push(resolved);
                }
            }
            set_reg(state, dst, result_ty, function, local_flags);
        }
        Instruction::CallNativeDirect { dst, .. } => {
            widen_dynamic_locals(state, function);
            set_reg(state, dst, SoundType::UNKNOWN, function, local_flags);
        }
        Instruction::CallMethod { site, dst, .. } => {
            // `CallMethod` either completes directly and jumps to `site.after`
            // (writing `dst`), or falls through into the written-out `InitMethodCall`.
            if let Some(msite) = function.method_sites.get(site.index()) {
                let receiver_ty = read_val(state, msite.receiver);
                let arg_types: Vec<SoundType> = function.site_arguments[msite.argument_range()]
                    .iter()
                    .map(|a| read_val(state, a.register()))
                    .collect();
                let mut result_ty = SoundType::UNKNOWN;
                if let Some(resolved) = ctx.resolve_method_call(
                    ip as u32,
                    unit_ref,
                    unit,
                    receiver_ty,
                    msite.method,
                    caller_class,
                    Some(&arg_types),
                ) {
                    if ctx.audit_holds.contains(resolved.needs) {
                        result_ty = resolved.result;
                    }
                    if let Some(calls) = out_calls.as_mut() {
                        calls.push(resolved);
                    }
                }
                let mut after_state = state.to_vec();
                widen_dynamic_locals(&mut after_state, function);
                if msite.receiver_taken
                    && msite.receiver.index() >= function.locals.len()
                    && let Some(slot) = after_state.get_mut(msite.receiver.index())
                {
                    *slot = UNDEF_TYPE;
                }
                set_reg(&mut after_state, dst, result_ty, function, local_flags);
                out.branch = Some((msite.after, Some(after_state)));
            }
        }

        // ---- Control flow ----
        Instruction::Jump { target } => {
            out.falls_through = false;
            out.branch = Some((target, None));
        }
        Instruction::JumpIf { target, .. }
        | Instruction::JumpIfNot { target, .. }
        | Instruction::BinaryBranch { target, .. }
        | Instruction::BinaryBranchIf { target, .. }
        | Instruction::BinaryIntBranch { target, .. }
        | Instruction::BinaryIntBranchIf { target, .. }
        | Instruction::DynamicInstanceOfBranch { target, .. }
        | Instruction::DynamicInstanceOfBranchIf { target, .. }
        | Instruction::GuardBuiltin { call: target, .. } => {
            out.branch = Some((target, None));
        }
        Instruction::JumpIfNull { value, target } => {
            if can_track_reg(value, function, local_flags) {
                let cur = state[value.index()];
                let mut null_state = state.to_vec();
                let null_or_undef =
                    SoundMask(cur.mask.0 & (SoundMask::NULL.0 | SoundMask::UNDEF.0));
                null_state[value.index()] = if null_or_undef != SoundMask::NONE {
                    SoundType {
                        mask: null_or_undef,
                        class: ClassBound::Any,
                    }
                } else {
                    SoundType::NULL
                };
                out.branch = Some((target, Some(null_state)));

                let mut non_null_state = state.to_vec();
                let stripped = SoundMask(cur.mask.0 & !(SoundMask::NULL.0 | SoundMask::UNDEF.0));
                if stripped != SoundMask::NONE {
                    non_null_state[value.index()] = SoundType {
                        mask: stripped,
                        class: cur.class,
                    };
                }
                out.fallthrough_narrow = Some(non_null_state);
            } else {
                out.branch = Some((target, None));
            }
        }
        Instruction::JumpIfNotNull { value, target } => {
            if can_track_reg(value, function, local_flags) {
                let cur = state[value.index()];
                let mut non_null_state = state.to_vec();
                let stripped = SoundMask(cur.mask.0 & !(SoundMask::NULL.0 | SoundMask::UNDEF.0));
                if stripped != SoundMask::NONE {
                    non_null_state[value.index()] = SoundType {
                        mask: stripped,
                        class: cur.class,
                    };
                }
                out.branch = Some((target, Some(non_null_state)));

                let mut null_state = state.to_vec();
                let null_or_undef =
                    SoundMask(cur.mask.0 & (SoundMask::NULL.0 | SoundMask::UNDEF.0));
                null_state[value.index()] = if null_or_undef != SoundMask::NONE {
                    SoundType {
                        mask: null_or_undef,
                        class: ClassBound::Any,
                    }
                } else {
                    SoundType::NULL
                };
                out.fallthrough_narrow = Some(null_state);
            } else {
                out.branch = Some((target, None));
            }
        }
        Instruction::InstanceOfBranch { src, class, target } => {
            // Jumps to `target` when FALSE; falls through when TRUE.
            out.branch = Some((target, None));
            if can_track_reg(src, function, local_flags) {
                let narrowed_ty = ctx.resolve_instanceof_class(unit, class, caller_class);
                let mut true_state = state.to_vec();
                true_state[src.index()] = narrowed_ty;
                out.fallthrough_narrow = Some(true_state);
            }
        }
        Instruction::InstanceOfBranchIf { src, class, target } => {
            // Jumps to `target` when TRUE; falls through when FALSE.
            if can_track_reg(src, function, local_flags) {
                let narrowed_ty = ctx.resolve_instanceof_class(unit, class, caller_class);
                let mut true_state = state.to_vec();
                true_state[src.index()] = narrowed_ty;
                out.branch = Some((target, Some(true_state)));
            } else {
                out.branch = Some((target, None));
            }
        }
        Instruction::ReceiveDefault { param, passed } => {
            if can_track_reg(param, function, local_flags) {
                let mut passed_state = state.to_vec();
                let cur = passed_state[param.index()];
                let stripped = SoundMask(cur.mask.0 & !SoundMask::UNDEF.0);
                if stripped != SoundMask::NONE {
                    passed_state[param.index()] = SoundType {
                        mask: stripped,
                        class: cur.class,
                    };
                }
                out.branch = Some((passed, Some(passed_state)));

                let mut omitted_state = state.to_vec();
                omitted_state[param.index()] = UNDEF_TYPE;
                out.fallthrough_narrow = Some(omitted_state);
            } else {
                out.branch = Some((passed, None));
            }
        }

        Instruction::IterStart { end, .. } => {
            out.branch = Some((end, None));
        }
        Instruction::IterStartReference { subject, end, .. } => {
            set_reg(state, subject, SoundType::UNKNOWN, function, local_flags);
            out.branch = Some((end, None));
        }
        Instruction::IterNext { value, end, .. } => {
            // Note: on jump to `end`, `value` is NOT overwritten; on fallthrough it is.
            out.branch = Some((end, Some(state.to_vec())));
            set_reg(state, value, SoundType::UNKNOWN, function, local_flags);
        }
        Instruction::IterNextKeyed {
            key, value, end, ..
        } => {
            out.branch = Some((end, Some(state.to_vec())));
            set_reg(state, key, SoundType::UNKNOWN, function, local_flags);
            set_reg(state, value, SoundType::UNKNOWN, function, local_flags);
        }

        Instruction::FinallyCall {
            state: st_reg,
            target,
            ..
        } => {
            set_reg(state, st_reg, SoundType::UNKNOWN, function, local_flags);
            out.falls_through = false;
            out.branch = Some((target, None));
        }
        Instruction::FinallyReturn { .. } => {
            out.falls_through = false;
            out.is_finally_return = true;
        }

        // ---- Terminators ----
        Instruction::Return { src } | Instruction::ReturnChecked { source: src } => {
            out.falls_through = false;
            out.returned = Some(read_val(state, src));
        }
        Instruction::ReturnReference { src } => {
            out.falls_through = false;
            out.returned = Some(read_val(state, src));
        }
        Instruction::ReturnNull => {
            out.falls_through = false;
            out.returned = Some(SoundType::NULL);
        }
        Instruction::GeneratorReturn { .. }
        | Instruction::ReturnMissing
        | Instruction::Throw { .. }
        | Instruction::Exit { .. }
        | Instruction::Raise { .. } => {
            out.falls_through = false;
        }
    }

    out
}

fn path_base_type(
    ctx: &ResolveContext<'_>,
    base: PathBase,
    state: &[SoundType],
    caller_class: Option<ClassRef>,
) -> Option<SoundType> {
    match base {
        PathBase::This { .. } => Some(this_sound_type(ctx, caller_class)),
        PathBase::Register(r) => Some(read_val(state, r)),
        _ => None,
    }
}

fn this_sound_type(ctx: &ResolveContext<'_>, caller_class: Option<ClassRef>) -> SoundType {
    match caller_class {
        Some(cref) => class_sound_type(cref, ctx.hierarchy),
        None => SoundType {
            mask: SoundMask::OBJECT,
            class: ClassBound::Any,
        },
    }
}

fn closure_sound_type(ctx: &ResolveContext<'_>) -> SoundType {
    ctx.hierarchy
        .class_ref("closure")
        .map(SoundType::exact_class)
        .unwrap_or(SoundType {
            mask: SoundMask::OBJECT,
            class: ClassBound::Any,
        })
}

fn generator_sound_type(ctx: &ResolveContext<'_>) -> SoundType {
    ctx.hierarchy
        .class_ref("generator")
        .map(SoundType::within_class)
        .unwrap_or(SoundType {
            mask: SoundMask::OBJECT,
            class: ClassBound::Any,
        })
}

fn constant_sound_type(constant: &Constant) -> SoundType {
    match constant {
        Constant::Null => SoundType::NULL,
        Constant::Bool(true) => SoundType {
            mask: SoundMask::TRUE,
            class: ClassBound::Any,
        },
        Constant::Bool(false) => SoundType {
            mask: SoundMask::FALSE,
            class: ClassBound::Any,
        },
        Constant::Int(_) => SoundType::INT,
        Constant::Float(_) => SoundType::FLOAT,
        Constant::String(_) => SoundType::STRING,
        Constant::Array(_) => SoundType::ARRAY,
    }
}

fn binary_sound_type(op: BinaryOp, lhs: SoundType, rhs: SoundType) -> SoundType {
    match op {
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Identical
        | BinaryOp::NotIdentical
        | BinaryOp::Less
        | BinaryOp::LessEqual
        | BinaryOp::Greater
        | BinaryOp::GreaterEqual
        | BinaryOp::LogicalXor => SoundType::BOOL,
        BinaryOp::Spaceship => SoundType::INT,
        BinaryOp::Concat => SoundType::STRING,
        BinaryOp::Coalesce => {
            let stripped = SoundMask(lhs.mask.0 & !(SoundMask::NULL.0 | SoundMask::UNDEF.0));
            if stripped == SoundMask::NONE {
                rhs
            } else if !lhs.mask.intersects(SoundMask::NULL.union(SoundMask::UNDEF)) {
                lhs
            } else {
                SoundType {
                    mask: stripped,
                    class: lhs.class,
                }
                .union(rhs)
            }
        }
        BinaryOp::Add => {
            let may_obj =
                lhs.mask.intersects(SoundMask::OBJECT) || rhs.mask.intersects(SoundMask::OBJECT);
            let may_arr =
                lhs.mask.intersects(SoundMask::ARRAY) && rhs.mask.intersects(SoundMask::ARRAY);
            if lhs.mask == SoundMask::ARRAY && rhs.mask == SoundMask::ARRAY {
                return SoundType::ARRAY;
            }
            let mut mask = if !may_obj
                && !may_arr
                && (lhs.mask == SoundMask::FLOAT || rhs.mask == SoundMask::FLOAT)
            {
                SoundMask::FLOAT
            } else {
                SoundMask::INT.union(SoundMask::FLOAT)
            };
            if may_arr {
                mask = mask.union(SoundMask::ARRAY);
            }
            if may_obj {
                // GMP objects overload arithmetic operators.
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
        BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Power => {
            let may_obj =
                lhs.mask.intersects(SoundMask::OBJECT) || rhs.mask.intersects(SoundMask::OBJECT);
            let mut mask =
                if !may_obj && (lhs.mask == SoundMask::FLOAT || rhs.mask == SoundMask::FLOAT) {
                    SoundMask::FLOAT
                } else {
                    SoundMask::INT.union(SoundMask::FLOAT)
                };
            if may_obj {
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
        BinaryOp::Modulo | BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            let may_obj =
                lhs.mask.intersects(SoundMask::OBJECT) || rhs.mask.intersects(SoundMask::OBJECT);
            let mut mask = SoundMask::INT;
            if may_obj {
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
        BinaryOp::BitwiseAnd | BinaryOp::BitwiseOr | BinaryOp::BitwiseXor => {
            let may_obj =
                lhs.mask.intersects(SoundMask::OBJECT) || rhs.mask.intersects(SoundMask::OBJECT);
            if lhs.mask == SoundMask::STRING && rhs.mask == SoundMask::STRING {
                return SoundType::STRING;
            }
            let mut mask = SoundMask::INT;
            if lhs.mask.intersects(SoundMask::STRING) && rhs.mask.intersects(SoundMask::STRING) {
                mask = mask.union(SoundMask::STRING);
            }
            if may_obj {
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
    }
}

fn unary_sound_type(op: UnaryOp, src: SoundType) -> SoundType {
    let may_obj = src.mask.intersects(SoundMask::OBJECT);
    match op {
        UnaryOp::Not => SoundType::BOOL,
        UnaryOp::Plus => {
            let mut mask = if !may_obj && src.mask == SoundMask::INT {
                SoundMask::INT
            } else if !may_obj && src.mask == SoundMask::FLOAT {
                SoundMask::FLOAT
            } else {
                SoundMask::INT.union(SoundMask::FLOAT)
            };
            if may_obj {
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
        UnaryOp::Negate => {
            // Note: `-PHP_INT_MIN` overflows to `float`.
            let mut mask = if !may_obj && src.mask == SoundMask::FLOAT {
                SoundMask::FLOAT
            } else {
                SoundMask::INT.union(SoundMask::FLOAT)
            };
            if may_obj {
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
        UnaryOp::BitwiseNot => {
            if src.mask == SoundMask::STRING {
                return SoundType::STRING;
            }
            let mut mask = SoundMask::INT;
            if src.mask.intersects(SoundMask::STRING) {
                mask = mask.union(SoundMask::STRING);
            }
            if may_obj {
                mask = mask.union(SoundMask::OBJECT);
            }
            SoundType {
                mask,
                class: ClassBound::Any,
            }
        }
    }
}

fn cast_sound_type(to: CastKind, src: SoundType) -> SoundType {
    match to {
        CastKind::Bool => SoundType::BOOL,
        CastKind::Int => SoundType::INT,
        CastKind::Float => SoundType::FLOAT,
        CastKind::String => SoundType::STRING,
        CastKind::Array => SoundType::ARRAY,
        CastKind::Object => SoundType {
            mask: SoundMask::OBJECT,
            class: if src.mask == SoundMask::OBJECT {
                src.class
            } else {
                ClassBound::Any
            },
        },
        CastKind::Void => SoundType::NULL,
    }
}

fn inc_dec_sound_type(mode: IncDec, before: SoundType) -> SoundType {
    let is_inc = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
    let numeric = SoundMask::INT.union(SoundMask::FLOAT);
    if before.mask == SoundMask::FLOAT {
        return SoundType::FLOAT;
    }
    if before.mask != SoundMask::NONE && (before.mask.0 & !numeric.0) == 0 {
        return SoundType {
            mask: numeric,
            class: ClassBound::Any,
        };
    }
    if before.mask == SoundMask::NULL {
        return if is_inc {
            SoundType::INT
        } else {
            SoundType::NULL
        };
    }
    if before.mask == SoundMask::STRING {
        return SoundType {
            mask: numeric.union(SoundMask::STRING),
            class: ClassBound::Any,
        };
    }
    SoundType::UNKNOWN
}

fn builtin_unary_sound_type(op: BuiltinUnary) -> SoundType {
    match op {
        BuiltinUnary::Strlen
        | BuiltinUnary::Count
        | BuiltinUnary::Sizeof
        | BuiltinUnary::ObjectId => SoundType::INT,
        BuiltinUnary::TypeCheck(_) | BuiltinUnary::IsResource | BuiltinUnary::Defined => {
            SoundType::BOOL
        }
        BuiltinUnary::GetType | BuiltinUnary::GetClass | BuiltinUnary::StrToLower => {
            SoundType::STRING
        }
    }
}

/// Mirrors `crates/vm/src/fast_tier/lower.rs::is_fast_candidate` so `.phb`
/// images carry precomputed Tier-2 eligibility in [`FunctionFlags::FAST_CANDIDATE`].
fn is_fast_candidate(function: &Function, self_local: Option<u32>) -> bool {
    if (function.registers as usize) > MAX_FAST_REGS
        || function.instructions.is_empty()
        || function.instructions.len() > MAX_FAST_OPS
        || !function.handlers.is_empty()
        || function.is_generator
        || function.returns_reference
        || function.observes_arguments
    {
        return false;
    }

    let insts = &function.instructions;
    for (ip, &inst) in insts.iter().enumerate() {
        if let Some(target) = fast_jump_target(inst) {
            let start = target.index();
            if start <= ip
                && insts[start..=ip]
                    .iter()
                    .all(|&op| is_supported_in_fast_loop(function, op))
            {
                return true;
            }
        }
    }

    if let Some(local_id) = self_local {
        let mut calls_self = false;
        let mut has_unsupported = false;
        for &inst in insts.iter() {
            if let Instruction::CallDirect {
                function: declared,
                count,
                ..
            } = inst
                && declared.0 == local_id
                && count <= 16
            {
                calls_self = true;
            }
            if !is_supported_in_fast_loop(function, inst) {
                has_unsupported = true;
                break;
            }
        }
        if calls_self && !has_unsupported {
            return true;
        }
    }

    false
}

fn fast_jump_target(inst: Instruction) -> Option<Target> {
    match inst {
        Instruction::Jump { target }
        | Instruction::JumpIf { target, .. }
        | Instruction::JumpIfNot { target, .. }
        | Instruction::JumpIfNull { target, .. }
        | Instruction::JumpIfNotNull { target, .. }
        | Instruction::BinaryBranch { target, .. }
        | Instruction::BinaryBranchIf { target, .. }
        | Instruction::BinaryIntBranch { target, .. }
        | Instruction::BinaryIntBranchIf { target, .. } => Some(target),
        _ => None,
    }
}

fn is_supported_in_fast_loop(function: &Function, inst: Instruction) -> bool {
    match inst {
        Instruction::Nop
        | Instruction::LoadInt { .. }
        | Instruction::LoadBool { .. }
        | Instruction::LoadNull { .. }
        | Instruction::LoadConst { .. }
        | Instruction::AssignConst { .. }
        | Instruction::NewArray { .. }
        | Instruction::Move { .. }
        | Instruction::Assign { .. }
        | Instruction::AssignTake { .. }
        | Instruction::Clear { .. }
        | Instruction::IncDecLocal { .. }
        | Instruction::Jump { .. }
        | Instruction::JumpIf { .. }
        | Instruction::JumpIfNot { .. }
        | Instruction::JumpIfNull { .. }
        | Instruction::JumpIfNotNull { .. }
        | Instruction::AppendLocal { .. }
        | Instruction::Return { .. }
        | Instruction::ReturnNull => true,
        Instruction::Binary { op, .. } => matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Modulo
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Identical
                | BinaryOp::NotIdentical
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ),
        Instruction::BinaryInt { op, .. } => matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Modulo
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Identical
                | BinaryOp::NotIdentical
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ),
        Instruction::AssignOp { op, local, src } => {
            matches!(op, BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply)
                || (op == BinaryOp::Concat && local != src)
        }
        Instruction::Unary { op, .. } => {
            matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitwiseNot)
        }
        Instruction::Cast { to, .. } => {
            matches!(to, CastKind::Int | CastKind::Float | CastKind::Bool)
        }
        Instruction::BuiltinUnary {
            op: BuiltinUnary::Strlen,
            ..
        } => true,
        Instruction::BinaryBranch { op, .. } | Instruction::BinaryBranchIf { op, .. } => matches!(
            op,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Identical
                | BinaryOp::NotIdentical
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ),
        Instruction::BinaryIntBranch { op, .. } | Instruction::BinaryIntBranchIf { op, .. } => {
            matches!(
                op,
                BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Identical
                    | BinaryOp::NotIdentical
            )
        }
        Instruction::FetchIndex { mode, .. } => mode != FetchMode::Destructure,
        Instruction::Fetch { path, mode, .. } => {
            mode != FetchMode::Destructure
                && function.paths.get(path.index()).is_some_and(|def| {
                    matches!(def.base, PathBase::Register(_))
                        && matches!(def.steps.as_slice(), [Step::Index(_), Step::Index(_)])
                })
        }
        Instruction::AssignPath { path, .. } => {
            function.paths.get(path.index()).is_some_and(|def| {
                matches!(def.base, PathBase::Register(_))
                    && matches!(
                        def.steps.as_slice(),
                        [Step::Index(_)] | [Step::Index(_), Step::Index(_)]
                    )
            })
        }
        Instruction::UnsetPath { path } => function.paths.get(path.index()).is_some_and(|def| {
            matches!(def.base, PathBase::Register(_))
                && matches!(def.steps.as_slice(), [Step::Index(_)])
        }),
        Instruction::AssignOpPath {
            op: BinaryOp::Add,
            path,
            ..
        } => function.paths.get(path.index()).is_some_and(|def| {
            matches!(def.base, PathBase::Register(_))
                && matches!(def.steps.as_slice(), [Step::Index(_)])
        }),
        Instruction::CallDirect { count, .. } => count <= 16,
        _ => false,
    }
}

/// Applies `FlowOutput` onto an existing [`FunctionBuildFacts`] (preserving
/// `locals` and existing `DYNAMIC_SCOPE` / `IS_GENERATOR` flags).
pub(crate) fn apply_flow_output(facts: &mut FunctionBuildFacts, out: FlowOutput) {
    if out.return_proven {
        facts.flags = facts.flags.union(FunctionFlags::RETURN_PROVEN);
    }
    if out.fast_candidate {
        facts.flags = facts.flags.union(FunctionFlags::FAST_CANDIDATE);
    }
    facts.block_starts = out.block_starts;
    facts.block_types = out.block_types;
    facts.calls = out.calls;
    facts.props = out.props;
    facts.returns = out.returns;
}
