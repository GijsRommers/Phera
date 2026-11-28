//! Closed-world call target and property slot resolution over sound bytecode
//! facts.
//!
//! Every resolved site carries the exact closed-world [`Assumptions`] it relies
//! on (`SEALED_NAMES`, `SEALED_HIERARCHY`, `UNIT_LOADED`), so both hybrid and
//! strict runtimes can safely test `audit.holds.contains(site.needs)` before
//! consuming it.

use phera_bytecode::facts::{
    Assumptions, CallFlags, ClassBound, ClassFlags, ClassRef, DeclUniqueness, FunctionBuildFacts,
    FunctionFlags, FunctionRef, MethodFlags, PropAccess, PropFlags, ResolvedCall, ResolvedProp,
    SoundMask, SoundType, UnitBuildFacts, UnitRef,
};
use phera_bytecode::{
    ClassBinding, CompiledUnit, Function, FunctionId, NameId, PathBase, Step, SymbolId, Visibility,
};

use super::CompiledWorldUnit;
use super::declarations::Declarations;
use super::hierarchy::Hierarchy;
use super::layout::{LinkedClassInfo, method_visible_from, property_visible_from};
use super::types::{
    is_class_subtype, is_exact_subtype, resolve_class_bound, sound_type_of_declared,
};

/// Shared read-only context for call and property resolution across all units.
pub(crate) struct ResolveContext<'a> {
    pub(crate) units: &'a [CompiledWorldUnit],
    pub(crate) declarations: &'a Declarations<'a>,
    pub(crate) hierarchy: &'a Hierarchy,
    pub(crate) linked_classes: &'a [LinkedClassInfo],
    pub(crate) audit_holds: Assumptions,
    /// Optional previous-pass `UnitBuildFacts` for inter-procedural return types
    /// and `RETURN_PROVEN` propagation.
    pub(crate) prev_facts: Option<&'a [UnitBuildFacts]>,
}

impl<'a> ResolveContext<'a> {
    pub(crate) fn function_body(
        &self,
        fref: FunctionRef,
    ) -> Option<(&'a CompiledUnit, &'a Function)> {
        let unit = &self.units.get(fref.unit.0 as usize)?.unit;
        let func = if fref.function == 0 {
            &unit.main
        } else {
            unit.functions.get((fref.function - 1) as usize)?
        };
        Some((unit, func))
    }

    pub(crate) fn prev_fn_facts(&self, fref: FunctionRef) -> Option<&'a FunctionBuildFacts> {
        self.prev_facts?
            .get(fref.unit.0 as usize)?
            .functions
            .get(fref.function as usize)
    }

    /// Sound return type observable by callers of `fref` when calling it is
    /// valid under the current pass.
    pub(crate) fn callee_return_type(
        &self,
        fref: FunctionRef,
        callee_class: Option<ClassRef>,
    ) -> (SoundType, bool) {
        if let Some(prev) = self.prev_fn_facts(fref) {
            return (
                prev.returns,
                prev.flags.contains(FunctionFlags::RETURN_PROVEN),
            );
        }
        let Some((unit, func)) = self.function_body(fref) else {
            return (SoundType::UNKNOWN, false);
        };
        if func.is_generator {
            let gen_ty = self
                .hierarchy
                .class_ref("generator")
                .map(SoundType::within_class)
                .unwrap_or(SoundType {
                    mask: SoundMask::OBJECT,
                    class: ClassBound::Any,
                });
            return (gen_ty, false);
        }
        let ret = func.return_type.map_or(SoundType::UNKNOWN, |tid| {
            sound_type_of_declared(unit, tid, self.hierarchy, callee_class)
        });
        (ret, false)
    }

    /// Resolves `New { class: sym_id }` to its target `ClassRef` when unambiguous.
    pub(crate) fn resolve_new_class(
        &self,
        unit: &CompiledUnit,
        sym_id: SymbolId,
        caller_class: Option<ClassRef>,
    ) -> Option<(ClassRef, bool /* exact */, Assumptions)> {
        let sym = unit.symbols.get(sym_id.index())?;
        let lower = sym.name.trim_start_matches('\\').to_ascii_lowercase();
        match lower.as_str() {
            "self" => {
                let cref = caller_class?;
                // `new self()` creates an instance of the exact lexical class!
                Some((cref, true, Assumptions::NONE))
            }
            "parent" => {
                let cref =
                    caller_class.and_then(|c| self.hierarchy.classes.get(c.0 as usize)?.parent)?;
                Some((cref, true, Assumptions::NONE))
            }
            "static" => {
                let cref = caller_class?;
                let layout = self.hierarchy.classes.get(cref.0 as usize)?;
                if layout.flags.contains(ClassFlags::FINAL) {
                    Some((cref, true, Assumptions::NONE))
                } else if layout.flags.contains(ClassFlags::EFFECTIVELY_FINAL) {
                    Some((cref, true, Assumptions::SEALED_HIERARCHY))
                } else {
                    Some((cref, false, Assumptions::NONE))
                }
            }
            name if name.starts_with('\0') => None,
            name => {
                let cref = self.hierarchy.class_ref(name)?;
                let layout = self.hierarchy.classes.get(cref.0 as usize)?;
                match layout.decl {
                    DeclUniqueness::Unique { .. } => Some((cref, true, Assumptions::SEALED_NAMES)),
                    DeclUniqueness::Native => Some((cref, true, Assumptions::NONE)),
                    _ => None,
                }
            }
        }
    }

    /// Resolves same-unit `CallDirect`.
    pub(crate) fn resolve_call_direct(
        &self,
        at: u32,
        current_unit: UnitRef,
        function_id: FunctionId,
        arg_types: &[SoundType],
    ) -> Option<ResolvedCall> {
        let target = FunctionRef {
            unit: current_unit,
            function: function_id.0 + 1,
        };
        let (callee_unit, callee_fn) = self.function_body(target)?;
        let (result, return_proven) = self.callee_return_type(target, None);
        let mut flags = CallFlags::NONE;
        if check_positional_args_exact(callee_unit, callee_fn, arg_types, self.hierarchy, None) {
            flags = flags.union(CallFlags::ARGS_EXACT);
        }
        if return_proven {
            flags = flags.union(CallFlags::RETURN_PROVEN);
        }
        Some(ResolvedCall {
            at,
            target,
            needs: Assumptions::NONE,
            flags,
            result,
        })
    }

    /// Resolves `InitCall { function: sym_id }` + `Call` at `at`.
    pub(crate) fn resolve_named_call(
        &self,
        at: u32,
        current_unit: UnitRef,
        unit: &CompiledUnit,
        sym_id: SymbolId,
        arg_types: Option<&[SoundType]>,
    ) -> Option<ResolvedCall> {
        let symbol = unit.symbols.get(sym_id.index())?;
        let name = symbol.name.to_ascii_lowercase();
        let (target_name, fallback_used) = match &symbol.fallback {
            None => (name, false),
            Some(fb) => {
                let ns_decl = self.declarations.function(&name);
                let fb_lower = fb.to_ascii_lowercase();
                let fb_decl = self.declarations.function(&fb_lower);
                if ns_decl == DeclUniqueness::Unknown {
                    (fb_lower, true)
                } else if matches!(ns_decl, DeclUniqueness::Unique { .. })
                    && fb_decl == DeclUniqueness::Unknown
                {
                    (name, true)
                } else {
                    // Both namespace and global exist (load-order dependent).
                    return None;
                }
            }
        };

        let DeclUniqueness::Unique {
            unit: target_unit,
            hoisted,
        } = self.declarations.function(&target_name)
        else {
            return None;
        };
        let target = self.declarations.unique_function(&target_name)?;
        let (callee_unit, callee_fn) = self.function_body(target)?;

        let mut needs = Assumptions::NONE;
        if fallback_used || target_unit != current_unit || !hoisted {
            needs = needs.union(Assumptions::SEALED_NAMES);
        }
        if target_unit != current_unit || !hoisted {
            needs = needs.union(Assumptions::UNIT_LOADED);
        }

        let (result, return_proven) = self.callee_return_type(target, None);
        let mut flags = CallFlags::NONE;
        if let Some(args) = arg_types
            && check_positional_args_exact(callee_unit, callee_fn, args, self.hierarchy, None)
        {
            flags = flags.union(CallFlags::ARGS_EXACT);
        }
        if return_proven {
            flags = flags.union(CallFlags::RETURN_PROVEN);
        }

        Some(ResolvedCall {
            at,
            target,
            needs,
            flags,
            result,
        })
    }

    /// Resolves an instance method call (`CallMethod` or `InitMethodCall`..`Call`)
    /// at instruction index `at`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_method_call(
        &self,
        at: u32,
        current_unit: UnitRef,
        unit: &CompiledUnit,
        receiver_ty: SoundType,
        method_name_id: NameId,
        caller_class: Option<ClassRef>,
        arg_types: Option<&[SoundType]>,
    ) -> Option<ResolvedCall> {
        if receiver_ty.mask != SoundMask::OBJECT {
            return None;
        }
        let method_name = unit.names.get(method_name_id.index())?.to_ascii_lowercase();
        let (cref, exact) = match receiver_ty.class {
            ClassBound::Exact(c) => (c, true),
            ClassBound::Within(c) => (c, false),
            ClassBound::Any => return None,
        };

        // 1. PHP private method dispatch rule: inside `scope`, calling `$x->priv()`
        //    on any instance of `scope` (or subclass) always calls `scope::priv()`.
        if let Some(scope) = caller_class
            && is_class_subtype(cref, scope, self.hierarchy)
            && let Some(scope_info) = self.linked_classes.get(scope.0 as usize)
            && let Some(priv_method) = scope_info.methods.get(&method_name)
            && priv_method.declaring == scope
            && priv_method.flags.contains(MethodFlags::PRIVATE)
            && !priv_method.flags.contains(MethodFlags::ABSTRACT)
        {
            return self.finish_method_call(
                at,
                current_unit,
                priv_method.function,
                scope,
                Assumptions::NONE,
                arg_types,
            );
        }

        let layout = self.hierarchy.classes.get(cref.0 as usize)?;
        if !matches!(layout.decl, DeclUniqueness::Unique { .. }) {
            return None;
        }
        let info = self.linked_classes.get(cref.0 as usize)?;
        let method = info.methods.get(&method_name)?;
        if method.flags.contains(MethodFlags::ABSTRACT)
            || !method_visible_from(method.flags, method.declaring, caller_class, self.hierarchy)
        {
            return None;
        }

        let mut needs = Assumptions::NONE;
        if method.function.unit != current_unit {
            needs = needs.union(Assumptions::SEALED_NAMES);
            needs = needs.union(Assumptions::UNIT_LOADED);
        }

        if exact {
            // Exact receiver type guarantees no subclass override. If the class
            // itself is not `final` and not from a same-unit declaration,
            // `SEALED_NAMES` ensures the class name cannot refer to another class.
            if !layout.flags.contains(ClassFlags::FINAL) {
                needs = needs.union(Assumptions::SEALED_NAMES);
            }
            return self.finish_method_call(
                at,
                current_unit,
                method.function,
                method.declaring,
                needs,
                arg_types,
            );
        }

        // `Within(cref)`: either the method/class is declared `final`, or every
        // closed-world descendant inherits the exact same method body.
        if layout.flags.contains(ClassFlags::FINAL) || method.flags.contains(MethodFlags::FINAL) {
            if !layout.flags.contains(ClassFlags::FINAL) {
                needs = needs.union(Assumptions::SEALED_NAMES);
            }
            return self.finish_method_call(
                at,
                current_unit,
                method.function,
                method.declaring,
                needs,
                arg_types,
            );
        }

        if !info.methods_complete {
            return None;
        }
        for &desc in self.hierarchy.descendants_of(cref) {
            let desc_layout = self.hierarchy.classes.get(desc.0 as usize)?;
            if !desc_layout.kind.is_class() {
                continue;
            }
            if !matches!(desc_layout.decl, DeclUniqueness::Unique { .. }) {
                return None;
            }
            let desc_info = self.linked_classes.get(desc.0 as usize)?;
            if !desc_info.methods_complete {
                return None;
            }
            let desc_method = desc_info.methods.get(&method_name)?;
            if desc_method.function != method.function {
                return None;
            }
        }
        needs = needs.union(Assumptions::SEALED_NAMES);
        needs = needs.union(Assumptions::SEALED_HIERARCHY);
        self.finish_method_call(
            at,
            current_unit,
            method.function,
            method.declaring,
            needs,
            arg_types,
        )
    }

    /// Resolves `InitStaticCall { class, method, binding }` + `Call` at `at`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_static_call(
        &self,
        at: u32,
        current_unit: UnitRef,
        unit: &CompiledUnit,
        class_sym_id: SymbolId,
        method_sym_id: SymbolId,
        _binding: ClassBinding,
        caller_class: Option<ClassRef>,
        arg_types: Option<&[SoundType]>,
    ) -> Option<ResolvedCall> {
        let class_sym = unit.symbols.get(class_sym_id.index())?;
        let method_sym = unit.symbols.get(method_sym_id.index())?;
        let class_lower = class_sym.name.trim_start_matches('\\').to_ascii_lowercase();
        let method_name = method_sym.name.to_ascii_lowercase();

        let (target_cref, is_late_static) = match class_lower.as_str() {
            "self" => (caller_class?, false),
            "parent" => (
                caller_class.and_then(|c| self.hierarchy.classes.get(c.0 as usize)?.parent)?,
                false,
            ),
            "static" => (caller_class?, true),
            name if name.starts_with('\0') => return None,
            name => (self.hierarchy.class_ref(name)?, false),
        };

        let layout = self.hierarchy.classes.get(target_cref.0 as usize)?;
        if !matches!(layout.decl, DeclUniqueness::Unique { .. }) {
            return None;
        }
        let info = self.linked_classes.get(target_cref.0 as usize)?;
        let method = info.methods.get(&method_name)?;
        if method.flags.contains(MethodFlags::ABSTRACT)
            || !method_visible_from(method.flags, method.declaring, caller_class, self.hierarchy)
        {
            return None;
        }

        let mut needs = Assumptions::NONE;
        if method.function.unit != current_unit
            || !matches!(class_lower.as_str(), "self" | "parent")
        {
            needs = needs.union(Assumptions::SEALED_NAMES);
        }
        if method.function.unit != current_unit {
            needs = needs.union(Assumptions::UNIT_LOADED);
        }

        if is_late_static
            && !layout.flags.contains(ClassFlags::FINAL)
            && !method.flags.contains(MethodFlags::FINAL)
            && !method.flags.contains(MethodFlags::PRIVATE)
        {
            if !info.methods_complete {
                return None;
            }
            for &desc in self.hierarchy.descendants_of(target_cref) {
                let desc_layout = self.hierarchy.classes.get(desc.0 as usize)?;
                if !desc_layout.kind.is_class() {
                    continue;
                }
                if !matches!(desc_layout.decl, DeclUniqueness::Unique { .. }) {
                    return None;
                }
                let desc_info = self.linked_classes.get(desc.0 as usize)?;
                if !desc_info.methods_complete {
                    return None;
                }
                let desc_method = desc_info.methods.get(&method_name)?;
                if desc_method.function != method.function {
                    return None;
                }
            }
            needs = needs.union(Assumptions::SEALED_HIERARCHY);
        }

        self.finish_method_call(
            at,
            current_unit,
            method.function,
            method.declaring,
            needs,
            arg_types,
        )
    }

    fn finish_method_call(
        &self,
        at: u32,
        current_unit: UnitRef,
        target: FunctionRef,
        declaring_class: ClassRef,
        mut needs: Assumptions,
        arg_types: Option<&[SoundType]>,
    ) -> Option<ResolvedCall> {
        let (callee_unit, callee_fn) = self.function_body(target)?;
        if target.unit != current_unit {
            needs = needs.union(Assumptions::UNIT_LOADED);
        }
        let (result, return_proven) = self.callee_return_type(target, Some(declaring_class));
        let mut flags = CallFlags::NONE;
        if let Some(args) = arg_types
            && check_positional_args_exact(
                callee_unit,
                callee_fn,
                args,
                self.hierarchy,
                Some(declaring_class),
            )
        {
            flags = flags.union(CallFlags::ARGS_EXACT);
        }
        if return_proven {
            flags = flags.union(CallFlags::RETURN_PROVEN);
        }
        Some(ResolvedCall {
            at,
            target,
            needs,
            flags,
            result,
        })
    }

    /// Resolves a single-step instance property access (`$obj->prop` or
    /// `$this->prop`) at instruction index `at`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_property_path(
        &self,
        at: u32,
        current_unit: UnitRef,
        unit: &CompiledUnit,
        path_base: PathBase,
        steps: &[Step],
        base_ty: SoundType,
        caller_class: Option<ClassRef>,
        write_value_ty: Option<SoundType>,
    ) -> Option<ResolvedProp> {
        if !matches!(path_base, PathBase::This { .. } | PathBase::Register(_)) {
            return None;
        }
        let [Step::Property(prop_name_id)] = steps else {
            return None;
        };
        if base_ty.mask != SoundMask::OBJECT {
            return None;
        }
        let prop_name = unit.names.get(prop_name_id.index())?;
        let (cref, exact) = match base_ty.class {
            ClassBound::Exact(c) => (c, true),
            ClassBound::Within(c) => (c, false),
            ClassBound::Any => return None,
        };

        let layout = self.hierarchy.classes.get(cref.0 as usize)?;
        if !layout.flags.contains(ClassFlags::LAYOUT_EXACT) {
            return None;
        }
        let info = self.linked_classes.get(cref.0 as usize)?;

        // Match runtime `PropertyNames::resolve` precedence:
        // 1. Private property declared in `caller_class` itself.
        // 2. Visible non-private property on `cref`.
        let slot_idx = find_visible_slot(info, prop_name, caller_class, self.hierarchy)?;
        let slot = layout.slots.get(slot_idx)?;
        let meta = info.slot_meta.get(slot_idx)?;

        // Never treat hooked properties as plain direct slot accesses.
        if slot.flags.contains(PropFlags::HAS_HOOKS) {
            return None;
        }

        let mut needs = Assumptions::NONE;
        if let DeclUniqueness::Unique {
            unit: decl_unit, ..
        } = layout.decl
            && decl_unit != current_unit
        {
            needs = needs.union(Assumptions::SEALED_NAMES);
            needs = needs.union(Assumptions::UNIT_LOADED);
        }

        let mut magic_get = layout.flags.contains(ClassFlags::MAGIC_GET);
        let mut magic_set = layout.flags.contains(ClassFlags::MAGIC_SET);

        if !exact && !layout.flags.contains(ClassFlags::FINAL) {
            // Check every closed-world descendant keeps the exact same slot index,
            // key, and no hooks on that slot.
            for &desc in self.hierarchy.descendants_of(cref) {
                let desc_layout = self.hierarchy.classes.get(desc.0 as usize)?;
                if !desc_layout.kind.is_class() {
                    continue;
                }
                if !desc_layout.flags.contains(ClassFlags::LAYOUT_EXACT) {
                    return None;
                }
                let desc_slot = desc_layout.slots.get(slot_idx)?;
                if desc_slot.key != slot.key || desc_slot.flags.contains(PropFlags::HAS_HOOKS) {
                    return None;
                }
                magic_get |= desc_layout.flags.contains(ClassFlags::MAGIC_GET);
                magic_set |= desc_layout.flags.contains(ClassFlags::MAGIC_SET);
            }
            needs = needs.union(Assumptions::SEALED_NAMES);
            needs = needs.union(Assumptions::SEALED_HIERARCHY);
        }

        let mut flags = PropAccess::NONE;
        if !magic_get {
            flags = flags.union(PropAccess::READ_EXACT);
        }
        let is_readonly = slot.flags.contains(PropFlags::READONLY);
        if !is_readonly {
            flags = flags.union(PropAccess::NON_READONLY);
        }
        let can_write_vis =
            !slot.flags.contains(PropFlags::SET_RESTRICTED) || caller_class == Some(meta.declaring);
        if !magic_set && !is_readonly && can_write_vis {
            flags = flags.union(PropAccess::WRITE_EXACT);
        }
        if let Some(rhs_ty) = write_value_ty
            && flags.contains(PropAccess::WRITE_EXACT)
        {
            let skip = match meta.declared_type {
                None => !slot.flags.contains(PropFlags::TYPED),
                Some((u, tid)) => is_exact_subtype(
                    rhs_ty,
                    &self.units[u.0 as usize].unit,
                    tid,
                    self.hierarchy,
                    Some(meta.declaring),
                ),
            };
            if skip {
                flags = flags.union(PropAccess::SKIP_TYPE_CHECK);
            }
        }

        let slot_u16 = u16::try_from(slot_idx).ok()?;
        Some(ResolvedProp {
            at,
            class: cref,
            slot: slot_u16,
            needs,
            flags,
            ty: slot.ty,
        })
    }

    /// Resolves `class` symbol of `InstanceOfBranch` / `InstanceOfBranchIf`.
    pub(crate) fn resolve_instanceof_class(
        &self,
        unit: &CompiledUnit,
        sym_id: SymbolId,
        caller_class: Option<ClassRef>,
    ) -> SoundType {
        let Some(sym) = unit.symbols.get(sym_id.index()) else {
            return SoundType {
                mask: SoundMask::OBJECT,
                class: ClassBound::Any,
            };
        };
        let lower = sym.name.trim_start_matches('\\').to_ascii_lowercase();
        resolve_class_bound(&lower, self.hierarchy, caller_class)
    }
}

fn find_visible_slot(
    info: &LinkedClassInfo,
    prop_name: &str,
    caller_class: Option<ClassRef>,
    hierarchy: &Hierarchy,
) -> Option<usize> {
    if let Some(scope) = caller_class
        && let Some((idx, _)) = info.slot_meta.iter().enumerate().find(|(_, m)| {
            m.name == prop_name && m.visibility == Visibility::Private && m.declaring == scope
        })
    {
        return Some(idx);
    }
    info.slot_meta
        .iter()
        .enumerate()
        .find(|(_, m)| {
            m.name == prop_name
                && m.visibility != Visibility::Private
                && property_visible_from(m.visibility, m.declaring, caller_class, hierarchy)
        })
        .map(|(idx, _)| idx)
}

fn check_positional_args_exact(
    callee_unit: &CompiledUnit,
    callee_fn: &Function,
    arg_types: &[SoundType],
    hierarchy: &Hierarchy,
    callee_class: Option<ClassRef>,
) -> bool {
    if callee_fn
        .parameters
        .iter()
        .any(|p| p.by_reference || p.variadic)
    {
        return false;
    }
    let required = callee_fn
        .parameters
        .iter()
        .filter(|p| !p.has_default)
        .count();
    if arg_types.len() < required || arg_types.len() > callee_fn.parameters.len() {
        return false;
    }
    for (param, &arg_ty) in callee_fn.parameters.iter().zip(arg_types.iter()) {
        if let Some(tid) = param.ty
            && !is_exact_subtype(arg_ty, callee_unit, tid, hierarchy, callee_class)
        {
            return false;
        }
    }
    true
}
