//! Build-time class linking: exact root-first instance property layouts
//! (`AotClassLayout::slots` + `ClassFlags::LAYOUT_EXACT`) and complete effective
//! method tables (`AotClassLayout::methods`).
//!
//! Mirrors the runtime linker in `crates/vm/src/classes/{linking,link_members,
//! trait_members,trait_methods,trait_adaptations,enums,class}.rs` using only
//! compiled bytecode. Whenever an ancestor or trait is `Native`, `Unknown`,
//! `Duplicate`, cyclic, or exhibits an ambiguous/conflicting override, linking
//! stays conservative (`LAYOUT_EXACT` clear, `slots` empty, `methods_complete`
//! false) so downstream specialization never relies on an uncertain slot or
//! method entry.

use std::collections::{BTreeMap, BTreeSet};

use phera_bytecode::facts::{
    AotMethod, AotPropSlot, ClassFlags, ClassRef, DeclUniqueness, FunctionRef, MethodFlags,
    PropFlags, SoundType, UnitRef,
};
use phera_bytecode::{
    ClassDeclaration, ClassKind, CompiledUnit, EnumBacking, MethodDeclaration, TraitAdaptation,
    TypeId, Visibility,
};

use super::CompiledWorldUnit;
use super::declarations::Declarations;
use super::hierarchy::Hierarchy;
use super::types::sound_type_of_declared;

/// Extra per-class build-time linker metadata retained for call and property
/// resolution (not serialized directly into `.phb`).
#[derive(Debug, Clone, Default)]
pub(crate) struct LinkedClassInfo {
    /// Effective method table keyed by lowercased method name.
    pub(crate) methods: BTreeMap<String, LinkedMethod>,
    /// True when the entire parent + trait chain was uniquely resolved in the
    /// build, so `methods` contains every inherited and trait-composed user
    /// method (no unknown/native ancestor could supply an unseen method).
    pub(crate) methods_complete: bool,
    /// Per-slot extra metadata aligned 1:1 with `AotClassLayout::slots` when
    /// `ClassFlags::LAYOUT_EXACT` is set.
    pub(crate) slot_meta: Vec<LinkedSlotMeta>,
}

#[derive(Debug, Clone)]
pub(crate) struct LinkedMethod {
    pub(crate) name: String,
    pub(crate) function: FunctionRef,
    pub(crate) declaring: ClassRef,
    pub(crate) flags: MethodFlags,
    /// Unit and `FunctionId` of the original body (for trait diamond identity).
    pub(crate) source_body: FunctionRef,
}

#[derive(Debug, Clone)]
pub(crate) struct LinkedSlotMeta {
    /// Original unmangled property name (case-sensitive).
    pub(crate) name: String,
    pub(crate) visibility: Visibility,
    pub(crate) declaring: ClassRef,
    /// Declared bytecode type ID in `declaring_unit`, if any.
    pub(crate) declared_type: Option<(UnitRef, TypeId)>,
}

#[derive(Debug, Clone)]
struct LinkedProp {
    key: Vec<u8>,
    name: String,
    declaring: ClassRef,
    declaring_name: String,
    visibility: Visibility,
    set_visibility: Option<Visibility>,
    is_readonly: bool,
    has_hooks: bool,
    declared_type: Option<(UnitRef, TypeId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Done,
}

struct LinkResult {
    props: Option<Vec<LinkedProp>>,
    statics: Option<Vec<LinkedProp>>,
    methods: BTreeMap<String, LinkedMethod>,
    methods_complete: bool,
}

/// Populates `AotClassLayout::slots`, `ClassFlags::LAYOUT_EXACT`, and complete
/// inherited + trait-composed `AotClassLayout::methods` on `hierarchy`, and
/// returns per-class [`LinkedClassInfo`] aligned 1:1 with `hierarchy.classes`.
pub(crate) fn link_classes(
    units: &[CompiledWorldUnit],
    declarations: &Declarations<'_>,
    hierarchy: &mut Hierarchy,
) -> Vec<LinkedClassInfo> {
    let class_count = hierarchy.classes.len();
    // Map each Unique ClassRef to its (unit_index, class_id).
    let mut unique_site: Vec<Option<(usize, usize)>> = vec![None; class_count];
    for (name, sites) in &declarations.classes {
        if let Some(cref) = hierarchy.class_ref(name)
            && matches!(
                hierarchy.classes[cref.0 as usize].decl,
                DeclUniqueness::Unique { .. }
            )
            && let [only] = sites.as_slice()
        {
            unique_site[cref.0 as usize] = Some((only.unit as usize, only.class.index()));
        }
    }

    let mut states = vec![VisitState::Unvisited; class_count];
    let mut results: Vec<Option<LinkResult>> = (0..class_count).map(|_| None).collect();

    for idx in 0..class_count {
        link_one(
            ClassRef(idx as u32),
            units,
            hierarchy,
            &unique_site,
            &mut states,
            &mut results,
        );
    }

    // Apply computed layouts and effective methods onto `hierarchy.classes`.
    let mut infos = Vec::with_capacity(class_count);
    for idx in 0..class_count {
        let cref = ClassRef(idx as u32);
        let Some(res) = results[idx].take() else {
            infos.push(LinkedClassInfo::default());
            continue;
        };
        let kind = hierarchy.classes[idx].kind;

        // Effective method table: write to `AotClassLayout::methods` when the
        // class is uniquely declared.
        if matches!(hierarchy.classes[idx].decl, DeclUniqueness::Unique { .. }) {
            let mut aot_methods: Vec<AotMethod> = res
                .methods
                .values()
                .map(|m| AotMethod {
                    name: m.name.as_str().into(),
                    function: m.function,
                    flags: m.flags,
                })
                .collect();
            aot_methods.sort_by(|a, b| a.name.cmp(&b.name));
            hierarchy.classes[idx].methods = aot_methods.into_boxed_slice();
        }

        // Instance property layout: only concrete/abstract classes, anonymous
        // classes, and enums carry object property slots (not traits/interfaces).
        let mut slot_meta = Vec::new();
        if kind.is_class()
            && let Some(defaults) = res.props
        {
            let mut slots = Vec::with_capacity(defaults.len());
            let mut seen_keys: BTreeSet<Vec<u8>> = BTreeSet::new();
            for prop in defaults {
                if !seen_keys.insert(prop.key.clone()) {
                    continue;
                }
                let ty = match prop.declared_type {
                    Some((u, tid)) => sound_type_of_declared(
                        &units[u.0 as usize].unit,
                        tid,
                        hierarchy,
                        Some(prop.declaring),
                    ),
                    None if kind == ClassKind::Enum => enum_slot_sound_type(
                        &prop.name,
                        unique_site[idx].and_then(|(u, c)| units[u].unit.classes[c].enum_backing),
                    ),
                    None => SoundType::UNKNOWN,
                };
                let mut flags = visibility_prop_flag(prop.visibility);
                if prop.is_readonly {
                    flags = flags.union(PropFlags::READONLY);
                }
                if prop.has_hooks {
                    flags = flags.union(PropFlags::HAS_HOOKS);
                }
                if prop.declared_type.is_some() || kind == ClassKind::Enum {
                    flags = flags.union(PropFlags::TYPED);
                }
                if prop.set_visibility.is_some() {
                    flags = flags.union(PropFlags::SET_RESTRICTED);
                }
                slots.push(AotPropSlot {
                    key: prop.key.into_boxed_slice(),
                    declaring: prop.declaring,
                    flags,
                    ty,
                });
                slot_meta.push(LinkedSlotMeta {
                    name: prop.name,
                    visibility: prop.visibility,
                    declaring: prop.declaring,
                    declared_type: prop.declared_type,
                });
            }
            hierarchy.classes[idx].slots = slots.into_boxed_slice();
            hierarchy.classes[idx].flags =
                hierarchy.classes[idx].flags.union(ClassFlags::LAYOUT_EXACT);
        }

        infos.push(LinkedClassInfo {
            methods: res.methods,
            methods_complete: res.methods_complete,
            slot_meta,
        });
        let _ = cref;
    }

    infos
}

fn link_one(
    cref: ClassRef,
    units: &[CompiledWorldUnit],
    hierarchy: &Hierarchy,
    unique_site: &[Option<(usize, usize)>],
    states: &mut [VisitState],
    results: &mut [Option<LinkResult>],
) {
    let idx = cref.0 as usize;
    match states[idx] {
        VisitState::Done => return,
        VisitState::Visiting => {
            // Cycle in broken code: leave result as `None` (conservative).
            return;
        }
        VisitState::Unvisited => {}
    }
    states[idx] = VisitState::Visiting;

    let Some((unit_idx, class_idx)) = unique_site[idx] else {
        states[idx] = VisitState::Done;
        return;
    };

    let unit = &units[unit_idx].unit;
    let decl = &unit.classes[class_idx];
    let unit_ref = UnitRef(unit_idx as u32);
    let parent_ref = hierarchy.classes[idx].parent;

    // 1. Recursively link parent first.
    if let Some(parent) = parent_ref {
        link_one(parent, units, hierarchy, unique_site, states, results);
    }

    // 2. Recursively link every trait used by this declaration.
    let mut trait_refs: Vec<ClassRef> = Vec::new();
    let mut traits_all_unique = true;
    let mut seen_traits = BTreeSet::new();
    for raw_name in decl.traits.iter().flat_map(|u| &u.names) {
        let key = raw_name.trim_start_matches('\\').to_ascii_lowercase();
        let Some(tref) = hierarchy.class_ref(&key) else {
            traits_all_unique = false;
            continue;
        };
        let tlayout = &hierarchy.classes[tref.0 as usize];
        if !matches!(tlayout.decl, DeclUniqueness::Unique { .. })
            || tlayout.kind != ClassKind::Trait
        {
            traits_all_unique = false;
            continue;
        }
        if seen_traits.insert(tref) {
            link_one(tref, units, hierarchy, unique_site, states, results);
            if results[tref.0 as usize].is_none() {
                // Trait had a cycle.
                traits_all_unique = false;
            }
            trait_refs.push(tref);
        }
    }

    // 3. Link methods (mirrors `linking.rs:353-442`, `trait_methods.rs`, `trait_adaptations.rs`).
    let (methods, methods_complete) = link_methods(
        cref,
        unit_ref,
        unit,
        decl,
        parent_ref,
        &trait_refs,
        traits_all_unique,
        hierarchy,
        results,
    );

    // 4. Link instance & static properties (mirrors `link_members.rs`, `trait_members.rs`, `enums.rs`).
    let (props, statics) = link_properties(
        cref,
        unit_ref,
        unit,
        decl,
        parent_ref,
        &trait_refs,
        traits_all_unique,
        hierarchy,
        results,
    );

    results[idx] = Some(LinkResult {
        props,
        statics,
        methods,
        methods_complete,
    });
    states[idx] = VisitState::Done;
}

#[allow(clippy::too_many_arguments)]
fn link_methods(
    cref: ClassRef,
    unit_ref: UnitRef,
    unit: &CompiledUnit,
    decl: &ClassDeclaration,
    parent_ref: Option<ClassRef>,
    trait_refs: &[ClassRef],
    traits_all_unique: bool,
    hierarchy: &Hierarchy,
    results: &[Option<LinkResult>],
) -> (BTreeMap<String, LinkedMethod>, bool) {
    let mut methods_complete = traits_all_unique;
    let mut methods: BTreeMap<String, LinkedMethod> = match parent_ref {
        None => BTreeMap::new(),
        Some(parent) => {
            let parent_layout = &hierarchy.classes[parent.0 as usize];
            if matches!(parent_layout.decl, DeclUniqueness::Unique { .. })
                && parent_layout.kind != ClassKind::Trait
                && parent_layout.kind != ClassKind::Interface
                && parent_layout.kind != ClassKind::Enum
                && let Some(parent_res) = &results[parent.0 as usize]
            {
                if !parent_res.methods_complete {
                    methods_complete = false;
                }
                parent_res.methods.clone()
            } else {
                methods_complete = false;
                BTreeMap::new()
            }
        }
    };

    // Own declared methods overwrite inherited ones.
    for method in &decl.methods {
        let Some(raw_name) = unit.names.get(method.name.index()) else {
            continue;
        };
        let key = raw_name.to_ascii_lowercase();
        let fref = FunctionRef {
            unit: unit_ref,
            function: method.function.0 + 1,
        };
        methods.insert(
            key.clone(),
            LinkedMethod {
                name: key,
                function: fref,
                declaring: cref,
                flags: method_flags(method),
                source_body: fref,
            },
        );
    }

    // Compose trait methods if all traits are known and unique.
    if !trait_refs.is_empty() {
        if traits_all_unique {
            if !compose_trait_methods(cref, decl, trait_refs, hierarchy, results, &mut methods) {
                methods_complete = false;
            }
        } else {
            methods_complete = false;
        }
    }

    // Enums have implicit native methods (`cases`, `from`, `tryFrom`), which are
    // not user `FunctionRef`s, so mark `methods_complete` conservatively if someone
    // queries an enum method not present in `methods`.
    (methods, methods_complete)
}

#[derive(Default)]
struct TraitRules {
    excluded: BTreeSet<(String, String)>,
    aliases: Vec<TraitAliasRule>,
}

struct TraitAliasRule {
    trait_name: String,
    method: String,
    alias: Option<String>,
    visibility: Option<Visibility>,
    is_final: bool,
}

fn compose_trait_methods(
    cref: ClassRef,
    decl: &ClassDeclaration,
    trait_refs: &[ClassRef],
    hierarchy: &Hierarchy,
    results: &[Option<LinkResult>],
    methods: &mut BTreeMap<String, LinkedMethod>,
) -> bool {
    let mut rules = TraitRules::default();
    let find_trait = |raw: &str| -> Option<(String, &LinkResult)> {
        let key = raw.trim_start_matches('\\').to_ascii_lowercase();
        let tref = trait_refs
            .iter()
            .copied()
            .find(|t| *hierarchy.classes[t.0 as usize].name == key)?;
        let res = results[tref.0 as usize].as_ref()?;
        Some((key, res))
    };

    for rule in decl.traits.iter().flat_map(|u| &u.adaptations) {
        match rule {
            TraitAdaptation::Precedence {
                trait_name,
                method,
                instead_of,
            } => {
                let key = method.to_ascii_lowercase();
                let Some((winner_name, winner_res)) = find_trait(trait_name) else {
                    return false;
                };
                if !winner_res.methods.contains_key(&key) {
                    return false;
                }
                for excluded_name in instead_of {
                    let Some((excl_key, _)) = find_trait(excluded_name) else {
                        return false;
                    };
                    if excl_key == winner_name {
                        return false;
                    }
                    if !rules.excluded.insert((excl_key, key.clone())) {
                        return false;
                    }
                }
            }
            TraitAdaptation::Alias {
                trait_name,
                method,
                alias,
                visibility,
                is_final,
            } => {
                let key = method.to_ascii_lowercase();
                let provider_name = if let Some(tname) = trait_name {
                    let Some((pkey, pres)) = find_trait(tname) else {
                        return false;
                    };
                    if !pres.methods.contains_key(&key) {
                        return false;
                    }
                    pkey
                } else {
                    let mut matches = trait_refs.iter().filter_map(|&tref| {
                        let res = results[tref.0 as usize].as_ref()?;
                        res.methods
                            .contains_key(&key)
                            .then(|| hierarchy.classes[tref.0 as usize].name.to_string())
                    });
                    let Some(first) = matches.next() else {
                        return false;
                    };
                    if matches.next().is_some() {
                        return false;
                    }
                    first
                };
                rules.aliases.push(TraitAliasRule {
                    trait_name: provider_name,
                    method: key,
                    alias: alias.as_ref().map(|a| a.to_ascii_lowercase()),
                    visibility: *visibility,
                    is_final: *is_final,
                });
            }
        }
    }

    let mut imported: Vec<(String, LinkedMethod)> = Vec::new();
    let mut requirements: Vec<LinkedMethod> = Vec::new();

    let mut add_import = |candidate: LinkedMethod| -> bool {
        let key = candidate.name.clone();
        if candidate.flags.contains(MethodFlags::ABSTRACT) {
            requirements.push(candidate);
            return true;
        }
        // Own class declaration always wins over trait methods.
        if methods
            .get(&key)
            .is_some_and(|owned| owned.declaring == cref)
        {
            return true;
        }
        if let Some((_, previous)) = imported.iter().find(|(n, _)| n == &key) {
            // Identical diamond trait method with matching visibility/finality.
            return previous.source_body == candidate.source_body
                && previous.flags == candidate.flags;
        }
        imported.push((key, candidate));
        true
    };

    for &tref in trait_refs {
        let provider_name = &*hierarchy.classes[tref.0 as usize].name;
        let Some(provider_res) = results[tref.0 as usize].as_ref() else {
            return false;
        };
        if !provider_res.methods_complete {
            return false;
        }
        for (key, method) in &provider_res.methods {
            let mut override_vis = None;
            let mut extra_final = false;
            for alias in rules
                .aliases
                .iter()
                .filter(|a| a.trait_name == provider_name && a.method == *key)
            {
                if let Some(new_name) = &alias.alias {
                    let mut m = method.clone();
                    m.name = new_name.clone();
                    m.declaring = cref;
                    m.flags = apply_method_modifiers(m.flags, alias.visibility, alias.is_final);
                    if !add_import(m) {
                        return false;
                    }
                } else {
                    override_vis = alias.visibility.or(override_vis);
                    extra_final |= alias.is_final;
                }
            }
            if !rules
                .excluded
                .contains(&(provider_name.to_owned(), key.clone()))
            {
                let mut m = method.clone();
                m.declaring = cref;
                m.flags = apply_method_modifiers(m.flags, override_vis, extra_final);
                if !add_import(m) {
                    return false;
                }
            }
        }
    }

    for (name, mut m) in imported {
        m.declaring = cref;
        methods.insert(name, m);
    }
    for mut req in requirements {
        let key = req.name.clone();
        if let std::collections::btree_map::Entry::Vacant(e) = methods.entry(key) {
            req.declaring = cref;
            e.insert(req);
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn link_properties(
    cref: ClassRef,
    unit_ref: UnitRef,
    unit: &CompiledUnit,
    decl: &ClassDeclaration,
    parent_ref: Option<ClassRef>,
    trait_refs: &[ClassRef],
    traits_all_unique: bool,
    hierarchy: &Hierarchy,
    results: &[Option<LinkResult>],
) -> (Option<Vec<LinkedProp>>, Option<Vec<LinkedProp>>) {
    if decl.kind == ClassKind::Interface || !traits_all_unique {
        return (None, None);
    }

    // Enums cannot extend classes or declare user/trait properties; they always
    // have exact slots (`name`, plus `value` if backed).
    if decl.kind == ClassKind::Enum {
        if parent_ref.is_some() || !decl.properties.is_empty() {
            return (None, None);
        }
        for &tref in trait_refs {
            let Some(tres) = results[tref.0 as usize].as_ref() else {
                return (None, None);
            };
            let (Some(tp), Some(ts)) = (&tres.props, &tres.statics) else {
                return (None, None);
            };
            if !tp.is_empty() || !ts.is_empty() {
                return (None, None);
            }
        }
        let mut defaults = vec![LinkedProp {
            key: b"name".to_vec(),
            name: "name".to_owned(),
            declaring: cref,
            declaring_name: decl.name.clone(),
            visibility: Visibility::Public,
            set_visibility: None,
            is_readonly: true,
            has_hooks: false,
            declared_type: None,
        }];
        if decl.enum_backing.is_some() {
            defaults.push(LinkedProp {
                key: b"value".to_vec(),
                name: "value".to_owned(),
                declaring: cref,
                declaring_name: decl.name.clone(),
                visibility: Visibility::Public,
                set_visibility: None,
                is_readonly: true,
                has_hooks: false,
                declared_type: None,
            });
        }
        return (Some(defaults), Some(Vec::new()));
    }

    // Inherit parent properties when parent is uniquely linked and exact.
    let (mut defaults, mut statics) = match parent_ref {
        None => (Vec::new(), Vec::new()),
        Some(parent) => {
            let parent_layout = &hierarchy.classes[parent.0 as usize];
            if !matches!(parent_layout.decl, DeclUniqueness::Unique { .. })
                || !matches!(parent_layout.kind, ClassKind::Class | ClassKind::Anonymous)
            {
                return (None, None);
            }
            let Some(pres) = results[parent.0 as usize].as_ref() else {
                return (None, None);
            };
            let (Some(pp), Some(ps)) = (&pres.props, &pres.statics) else {
                return (None, None);
            };
            (pp.clone(), ps.clone())
        }
    };

    // Process own declared properties in exact source order (`link_members.rs:128-230`).
    for prop in &decl.properties {
        let Some(prop_name) = unit.names.get(prop.name.index()) else {
            return (None, None);
        };
        let key = mangle_property_key(&decl.name, prop_name, prop.visibility);
        let has_hooks = prop
            .hooks
            .as_ref()
            .is_some_and(|h| h.get.is_some() || h.set.is_some());
        let mut linked = LinkedProp {
            key: key.clone(),
            name: prop_name.clone(),
            declaring: cref,
            declaring_name: decl.name.clone(),
            visibility: prop.visibility,
            set_visibility: prop.set_visibility,
            is_readonly: prop.is_readonly,
            has_hooks,
            declared_type: prop.ty.map(|tid| (unit_ref, tid)),
        };

        // Bail if static vs non-static conflict with inherited non-private property.
        let other = if prop.is_static { &defaults } else { &statics };
        if parent_ref.is_some()
            && other
                .iter()
                .any(|c| c.name == *prop_name && c.visibility != Visibility::Private)
        {
            return (None, None);
        }

        let target = if prop.is_static {
            &mut statics
        } else {
            &mut defaults
        };
        if let Some(inherited) = target.iter_mut().find(|entry| {
            entry.key == key
                || (entry.name == *prop_name
                    && entry.visibility != Visibility::Private
                    && prop.visibility != Visibility::Private)
        }) {
            // Inherit hooks from overridden property (`inherit_property_hooks`).
            linked.has_hooks |= inherited.has_hooks;
            *inherited = linked;
        } else {
            target.push(linked);
        }
    }

    // Compose traits in exact order (`trait_members.rs:100-241`).
    for &tref in trait_refs {
        let Some(tres) = results[tref.0 as usize].as_ref() else {
            return (None, None);
        };
        let (Some(tdefaults), Some(tstatics)) = (&tres.props, &tres.statics) else {
            return (None, None);
        };

        // Check static/non-static conflicts against opposite storage.
        for (source, opposite) in [(tdefaults, &statics), (tstatics, &defaults)] {
            for tprop in source {
                if opposite.iter().any(|e| {
                    e.name == tprop.name
                        && (e.declaring == cref || e.visibility != Visibility::Private)
                }) {
                    return (None, None);
                }
            }
        }

        for (source, target) in [(tdefaults, &mut defaults), (tstatics, &mut statics)] {
            for tprop in source {
                let key = if tprop.visibility == Visibility::Private {
                    mangle_property_key(&decl.name, &tprop.name, Visibility::Private)
                } else {
                    tprop.key.clone()
                };
                if let Some(existing) = target
                    .iter()
                    .find(|c| c.name == tprop.name && c.declaring == cref)
                {
                    if existing.has_hooks || tprop.has_hooks {
                        return (None, None);
                    }
                    if existing.visibility != tprop.visibility
                        || existing.is_readonly != tprop.is_readonly
                    {
                        return (None, None);
                    }
                    continue;
                }
                // If an inherited non-private property has the same name with a
                // different mangled key (e.g. parent protected, trait public),
                // stay conservative so slot positions cannot diverge.
                if target.iter().any(|c| {
                    c.name == tprop.name && c.visibility != Visibility::Private && c.key != key
                }) {
                    return (None, None);
                }
                let mut linked = tprop.clone();
                linked.key = key.clone();
                linked.declaring = cref;
                linked.declaring_name = decl.name.clone();
                if let Some(inherited) = target.iter_mut().find(|c| c.key == key) {
                    linked.has_hooks |= inherited.has_hooks;
                    *inherited = linked;
                } else {
                    target.push(linked);
                }
            }
        }
    }

    (Some(defaults), Some(statics))
}

fn mangle_property_key(class_name: &str, prop_name: &str, visibility: Visibility) -> Vec<u8> {
    match visibility {
        Visibility::Public => prop_name.as_bytes().to_vec(),
        Visibility::Protected => {
            let mut out = Vec::with_capacity(prop_name.len() + 3);
            out.extend_from_slice(b"\0*\0");
            out.extend_from_slice(prop_name.as_bytes());
            out
        }
        Visibility::Private => {
            let mut out = Vec::with_capacity(class_name.len() + prop_name.len() + 2);
            out.push(0);
            out.extend_from_slice(class_name.as_bytes());
            out.push(0);
            out.extend_from_slice(prop_name.as_bytes());
            out
        }
    }
}

fn enum_slot_sound_type(name: &str, backing: Option<EnumBacking>) -> SoundType {
    match name {
        "name" => SoundType::STRING,
        "value" => match backing {
            Some(EnumBacking::Int) => SoundType::INT,
            Some(EnumBacking::String) => SoundType::STRING,
            None => SoundType::UNKNOWN,
        },
        _ => SoundType::UNKNOWN,
    }
}

fn visibility_prop_flag(visibility: Visibility) -> PropFlags {
    match visibility {
        Visibility::Public => PropFlags::PUBLIC,
        Visibility::Protected => PropFlags::PROTECTED,
        Visibility::Private => PropFlags::PRIVATE,
    }
}

fn method_flags(method: &MethodDeclaration) -> MethodFlags {
    let mut flags = match method.visibility {
        Visibility::Public => MethodFlags::PUBLIC,
        Visibility::Protected => MethodFlags::PROTECTED,
        Visibility::Private => MethodFlags::PRIVATE,
    };
    if method.is_static {
        flags = flags.union(MethodFlags::STATIC);
    }
    if method.is_final {
        flags = flags.union(MethodFlags::FINAL);
    }
    if method.is_abstract {
        flags = flags.union(MethodFlags::ABSTRACT);
    }
    flags
}

fn apply_method_modifiers(
    mut flags: MethodFlags,
    visibility: Option<Visibility>,
    is_final: bool,
) -> MethodFlags {
    if let Some(vis) = visibility {
        let cleared =
            flags.0 & !(MethodFlags::PUBLIC.0 | MethodFlags::PROTECTED.0 | MethodFlags::PRIVATE.0);
        let vis_bit = match vis {
            Visibility::Public => MethodFlags::PUBLIC.0,
            Visibility::Protected => MethodFlags::PROTECTED.0,
            Visibility::Private => MethodFlags::PRIVATE.0,
        };
        flags = MethodFlags(cleared | vis_bit);
    }
    if is_final {
        flags = flags.union(MethodFlags::FINAL);
    }
    flags
}

/// Checks whether a property declared in `declaring` (at `visibility`) is
/// accessible from a caller method whose lexical class scope is `caller_class`.
pub(crate) fn property_visible_from(
    visibility: Visibility,
    declaring: ClassRef,
    caller_class: Option<ClassRef>,
    hierarchy: &Hierarchy,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => caller_class == Some(declaring),
        Visibility::Protected => caller_class.is_some_and(|caller| {
            super::types::is_class_subtype(caller, declaring, hierarchy)
                || super::types::is_class_subtype(declaring, caller, hierarchy)
        }),
    }
}

/// Checks whether a method declared in `declaring` with `flags` is accessible
/// from a caller whose lexical class scope is `caller_class`.
pub(crate) fn method_visible_from(
    flags: MethodFlags,
    declaring: ClassRef,
    caller_class: Option<ClassRef>,
    hierarchy: &Hierarchy,
) -> bool {
    if flags.contains(MethodFlags::PUBLIC) {
        true
    } else if flags.contains(MethodFlags::PRIVATE) {
        caller_class == Some(declaring)
    } else {
        caller_class.is_some_and(|caller| {
            super::types::is_class_subtype(caller, declaring, hierarchy)
                || super::types::is_class_subtype(declaring, caller, hierarchy)
        })
    }
}
