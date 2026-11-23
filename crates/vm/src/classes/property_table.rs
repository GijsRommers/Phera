//! Flat data-oriented property lookup table on [`Class`].
//!
//! Maps a process-interned case-sensitive [`PropKey`] directly to a compact
//! 16-byte [`PackedProp`] descriptor (slot index, visibility/hook/readonly/type
//! bitset, declaring class index) with zero per-instruction heap caches, zero
//! `RefCell` borrows, and zero string comparisons at runtime.

use std::rc::Rc;

use phera_bytecode::{ClassKind, NameKey, PropKey, TypeMask, Visibility};

use super::class::{Class, LinkedConstant};
use super::property_facts::PropertyFacts;
use super::{ClassIndex, ClassTable};

const EMPTY_KEY: u32 = u32::MAX;
const UNRESOLVED_OWNER: u32 = u32::MAX;

const VIS_MASK: u16 = 0b11;
const VIS_PUBLIC: u16 = 0;
const VIS_PROTECTED: u16 = 1;
const VIS_PRIVATE: u16 = 2;
const HAS_HOOKS: u16 = 1 << 2;
const IS_READONLY: u16 = 1 << 3;
const HAS_SET_VISIBILITY: u16 = 1 << 4;
const UNTYPED: u16 = 1 << 5;
const STATIC_COLLISION: u16 = 1 << 6;
const HAS_PRIVATE_SHADOW: u16 = 1 << 7;
const HAS_CLASS_TYPE: u16 = 1 << 8;

const TABLE_HAS_NATIVE_PROPS: u8 = 1 << 0;
const TABLE_HAS_MAGIC_GET: u8 = 1 << 1;
const TABLE_HAS_MAGIC_ISSET: u8 = 1 << 2;
const TABLE_HAS_STATICS: u8 = 1 << 3;
const TABLE_HASHED: u8 = 1 << 4;
const TABLE_HAS_MAGIC_SET: u8 = 1 << 5;

/// Compact 16-byte `Copy` descriptor for a declared instance property.
#[derive(Clone, Copy)]
pub(crate) struct PackedProp {
    key: u32,
    owner: u32,
    slot: u16,
    flags: u16,
    type_mask: TypeMask,
    _pad: u16,
}

impl PackedProp {
    const EMPTY: Self = Self {
        key: EMPTY_KEY,
        owner: UNRESOLVED_OWNER,
        slot: 0,
        flags: 0,
        type_mask: TypeMask(0),
        _pad: 0,
    };

    #[inline(always)]
    pub(crate) fn slot(self) -> usize {
        self.slot as usize
    }

    #[inline(always)]
    pub(crate) fn is_plain_readable(self) -> bool {
        self.flags & (HAS_HOOKS | STATIC_COLLISION) == 0
    }

    #[inline(always)]
    pub(crate) fn is_plain_writable(self) -> bool {
        self.flags & (HAS_HOOKS | IS_READONLY | HAS_SET_VISIBILITY | STATIC_COLLISION) == 0
    }

    #[inline(always)]
    pub(crate) fn allows_type(self, own: TypeMask) -> bool {
        self.flags & UNTYPED != 0 || self.type_mask.intersects(own)
    }

    #[inline(always)]
    pub(crate) fn allows_array(self) -> bool {
        self.allows_type(TypeMask::ARRAY)
    }

    #[inline(always)]
    pub(crate) fn allows_int(self) -> bool {
        self.allows_type(TypeMask::INT)
    }

    #[inline(always)]
    pub(crate) fn has_class_type(self) -> bool {
        self.flags & HAS_CLASS_TYPE != 0
    }

    #[inline(always)]
    fn is_private(self) -> bool {
        (self.flags & VIS_MASK) == VIS_PRIVATE
    }

    /// Whether code executing in `scope` may access this property.
    /// Returns `false` if unsure or denied, so the caller falls back to the VM.
    #[inline(always)]
    pub(crate) fn visible_to(self, scope: Option<ClassIndex>, classes: &ClassTable) -> bool {
        match self.flags & VIS_MASK {
            VIS_PUBLIC => true,
            VIS_PRIVATE => {
                self.owner != UNRESOLVED_OWNER
                    && scope.is_some_and(|scope| scope.number() == self.owner)
            }
            VIS_PROTECTED => {
                if self.owner == UNRESOLVED_OWNER {
                    return false;
                }
                let Some(scope) = scope else {
                    return false;
                };
                if scope.number() == self.owner {
                    return true;
                }
                protected_visible(scope, self.owner, classes)
            }
            _ => false,
        }
    }
}

#[cold]
#[inline(never)]
fn protected_visible(scope: ClassIndex, owner: u32, classes: &ClassTable) -> bool {
    let scope_class = classes.class(scope);
    let Some(owner_class) = classes.by_number(owner) else {
        return false;
    };
    scope_class.derives_from(owner_class) == Some(true)
        || owner_class.derives_from(scope_class) == Some(true)
}

/// Compact 8-byte `Copy` rule mapping a class-typed property slot to its
/// interned expected class [`NameKey`] slice in [`PropertyTable::class_type_keys`].
#[derive(Clone, Copy)]
struct ClassTypeRule {
    slot: u16,
    /// Bit 0: `1` for intersection (`A&B`), `0` for single/union (`A` / `A|B`).
    flags: u16,
    start: u16,
    len: u16,
}

/// Flat property lookup table built once per [`Class`] and shared across forks.
pub(crate) struct PropertyTable {
    slots: Box<[PackedProp]>,
    shadows: Box<[PackedProp]>,
    class_type_rules: Box<[ClassTypeRule]>,
    class_type_keys: Box<[NameKey]>,
    table_flags: u8,
}

impl PropertyTable {
    pub(crate) fn build(class: &Class) -> Self {
        let facts = class.property_facts();
        let mut table_flags = 0u8;
        if class.property_access.is_some() || phera_value::ClassHandle::has_native_properties(class)
        {
            table_flags |= TABLE_HAS_NATIVE_PROPS;
        }
        if facts.has(PropertyFacts::MAGIC_GET) {
            table_flags |= TABLE_HAS_MAGIC_GET;
        }
        if facts.has(PropertyFacts::MAGIC_ISSET) {
            table_flags |= TABLE_HAS_MAGIC_ISSET;
        }
        if facts.has(PropertyFacts::MAGIC_SET) {
            table_flags |= TABLE_HAS_MAGIC_SET;
        }
        if !class.static_properties.is_empty() {
            table_flags |= TABLE_HAS_STATICS;
        }
        if class.kind == ClassKind::Trait
            || class.defaults.is_empty()
            || class.defaults.len() > u16::MAX as usize
            || table_flags & TABLE_HAS_NATIVE_PROPS != 0
        {
            return Self {
                slots: Box::new([]),
                shadows: Box::new([]),
                class_type_rules: Box::new([]),
                class_type_keys: Box::new([]),
                table_flags,
            };
        }

        let keys = phera_bytecode::name_keys::prop_keys(
            class.defaults.iter().map(|prop| prop.name.as_str()),
        );
        let mut primary: Vec<PackedProp> = Vec::with_capacity(class.defaults.len());
        let mut shadows: Vec<PackedProp> = Vec::new();
        let mut class_type_rules: Vec<ClassTypeRule> = Vec::new();
        let mut class_type_keys: Vec<NameKey> = Vec::new();

        for (slot_index, (prop, key)) in class.defaults.iter().zip(keys.iter().copied()).enumerate()
        {
            let owner = resolve_owner(class, &prop.declaring);
            let mut flags = match prop.visibility {
                Visibility::Public => VIS_PUBLIC,
                Visibility::Protected => VIS_PROTECTED,
                Visibility::Private => VIS_PRIVATE,
            };
            if prop.hooks.is_some() {
                flags |= HAS_HOOKS;
            }
            if prop.is_readonly {
                flags |= IS_READONLY;
            }
            if prop.set_visibility.is_some() {
                flags |= HAS_SET_VISIBILITY;
            }
            let type_mask = match &prop.ty {
                None => {
                    flags |= UNTYPED;
                    TypeMask(0)
                }
                Some(ty) => {
                    if !ty.classes.is_empty() {
                        flags |= HAS_CLASS_TYPE;
                        push_class_type_rule(
                            class,
                            prop,
                            ty,
                            slot_index as u16,
                            &mut class_type_rules,
                            &mut class_type_keys,
                        );
                    }
                    ty.mask
                }
            };
            if !class.static_properties.is_empty()
                && class
                    .static_properties
                    .iter()
                    .any(|sp| sp.name == prop.name)
            {
                flags |= STATIC_COLLISION;
            }
            let packed = PackedProp {
                key: key.0,
                owner,
                slot: slot_index as u16,
                flags,
                type_mask,
                _pad: 0,
            };
            if let Some(existing) = primary.iter_mut().find(|entry| entry.key == key.0) {
                let mut prev = *existing;
                let had_shadow = prev.flags & HAS_PRIVATE_SHADOW != 0;
                if prev.is_private() {
                    prev.flags &= !HAS_PRIVATE_SHADOW;
                    shadows.push(prev);
                }
                let mut replacement = packed;
                if had_shadow || prev.is_private() {
                    replacement.flags |= HAS_PRIVATE_SHADOW;
                }
                *existing = replacement;
            } else {
                primary.push(packed);
            }
        }

        let class_type_rules = class_type_rules.into_boxed_slice();
        let class_type_keys = class_type_keys.into_boxed_slice();
        if primary.len() <= 8 {
            Self {
                slots: primary.into_boxed_slice(),
                shadows: shadows.into_boxed_slice(),
                class_type_rules,
                class_type_keys,
                table_flags,
            }
        } else {
            let size = (primary.len() * 2).next_power_of_two().max(16);
            let mask = size - 1;
            let mut table = vec![PackedProp::EMPTY; size].into_boxed_slice();
            for entry in primary {
                let mut index = start(entry.key, mask);
                while table[index].key != EMPTY_KEY {
                    index = (index + 1) & mask;
                }
                table[index] = entry;
            }
            table_flags |= TABLE_HASHED;
            Self {
                slots: table,
                shadows: shadows.into_boxed_slice(),
                class_type_rules,
                class_type_keys,
                table_flags,
            }
        }
    }

    /// Whether `object_class` satisfies the declared class/interface type of `slot`.
    /// Returns `false` for complex DNF/`static` declarations so the VM handles them.
    #[inline]
    pub(crate) fn allows_object_class(
        &self,
        slot: usize,
        object_class: &Class,
        classes: &ClassTable,
    ) -> bool {
        let Some(rule) = self
            .class_type_rules
            .iter()
            .find(|rule| rule.slot as usize == slot)
        else {
            return false;
        };
        let start = rule.start as usize;
        let end = start + rule.len as usize;
        let Some(keys) = self.class_type_keys.get(start..end) else {
            return false;
        };
        let derives = |&key: &NameKey| {
            classes
                .by_key(key)
                .is_some_and(|target| object_class.derives_from(target) == Some(true))
        };
        if rule.flags & 1 != 0 {
            keys.iter().all(derives)
        } else {
            keys.iter().any(derives)
        }
    }

    #[inline(always)]
    pub(crate) fn has_native_props(&self) -> bool {
        self.table_flags & TABLE_HAS_NATIVE_PROPS != 0
    }

    /// True when an unset/uninitialized declared property silently yields `null`
    /// in quiet (`??`, `isset`, `empty`) modes without invoking `__isset`/`__get`.
    #[inline(always)]
    pub(crate) fn quiet_null_when_missing(&self) -> bool {
        self.table_flags & (TABLE_HAS_MAGIC_GET | TABLE_HAS_MAGIC_ISSET | TABLE_HAS_NATIVE_PROPS)
            == 0
    }

    /// True when a write may fill a declared property that holds no value.
    /// With `__set`, a property that was `unset()` sends its next write to
    /// `__set` instead, so such a write must take the general path, which
    /// tells an unset property from one that was never initialized.
    #[inline(always)]
    pub(crate) fn fills_missing_directly(&self) -> bool {
        self.table_flags & TABLE_HAS_MAGIC_SET == 0
    }

    /// Resolve a declared property by interned [`PropKey`] and caller scope.
    /// Handles lexical private shadowing identically to PHP.
    #[inline(always)]
    pub(crate) fn resolve(&self, key: PropKey, scope: Option<ClassIndex>) -> Option<PackedProp> {
        let entry = self.find_primary(key.0)?;
        if entry.flags & HAS_PRIVATE_SHADOW == 0 {
            return Some(entry);
        }
        if let Some(scope) = scope {
            let scope_num = scope.number();
            for &shadow in &self.shadows {
                if shadow.key == key.0 && shadow.owner == scope_num {
                    return Some(shadow);
                }
            }
        }
        Some(entry)
    }

    #[inline(always)]
    fn find_primary(&self, key: u32) -> Option<PackedProp> {
        let slots = &self.slots;
        if self.table_flags & TABLE_HASHED == 0 {
            for &entry in slots {
                if entry.key == key {
                    return Some(entry);
                }
            }
            None
        } else {
            let mask = slots.len() - 1;
            let mut index = start(key, mask);
            loop {
                let entry = slots[index];
                if entry.key == key {
                    return Some(entry);
                }
                if entry.key == EMPTY_KEY {
                    return None;
                }
                index = (index + 1) & mask;
            }
        }
    }
}

#[inline(always)]
fn start(key: u32, mask: usize) -> usize {
    (key.wrapping_mul(0x9E37_79B9) >> 16) as usize & mask
}

fn resolve_owner(class: &Class, declaring: &str) -> u32 {
    let mut current = Some(class);
    while let Some(candidate) = current {
        if candidate.name == declaring {
            return candidate.index.number();
        }
        current = candidate.parent.as_deref();
    }
    UNRESOLVED_OWNER
}

fn resolve_parent_name<'a>(class: &'a Class, declaring: &str) -> Option<&'a str> {
    let mut current = Some(class);
    while let Some(candidate) = current {
        if candidate.name == declaring {
            return candidate.parent.as_deref().map(|p| p.name.as_str());
        }
        current = candidate.parent.as_deref();
    }
    None
}

fn push_class_type_rule(
    class: &Class,
    prop: &super::class::PropertyDefault,
    declared: &phera_bytecode::Type,
    slot: u16,
    rules: &mut Vec<ClassTypeRule>,
    keys_out: &mut Vec<NameKey>,
) {
    let display = declared.display.trim_start_matches('?');
    if display.contains(['(', ')']) {
        return;
    }
    let intersection = declared.intersection;
    let separator = if intersection { '&' } else { '|' };
    let mut names: Vec<&str> = Vec::new();
    for raw in display.split(separator) {
        let name = raw.trim();
        let lower_check = name.to_ascii_lowercase();
        match lower_check.as_str() {
            "null" | "false" | "true" | "bool" | "int" | "float" | "string" | "array"
            | "object" | "iterable" | "callable" | "mixed" => continue,
            "static" => return,
            "self" => names.push(prop.declaring.as_str()),
            "parent" => {
                let Some(parent) = resolve_parent_name(class, &prop.declaring) else {
                    return;
                };
                names.push(parent);
            }
            _ => names.push(name.trim_start_matches('\\')),
        }
    }
    if names.is_empty() || keys_out.len() + names.len() > u16::MAX as usize {
        return;
    }
    let start = keys_out.len() as u16;
    // Class keys number lowercased names (`ClassTable::by_key`).
    let lowercase: Vec<String> = names.iter().map(|name| name.to_ascii_lowercase()).collect();
    let interned = phera_bytecode::name_keys::keys(lowercase.iter().map(String::as_str));
    let len = interned.len() as u16;
    keys_out.extend(interned.iter().copied());
    rules.push(ClassTypeRule {
        slot,
        flags: u16::from(intersection),
        start,
        len,
    });
}

/// One entry in a class's flat constant lookup table.
#[derive(Clone)]
pub(crate) struct ConstantSlot {
    key: u32,
    pub(crate) owner: u32,
    pub(crate) constant: Rc<LinkedConstant>,
}

/// Flat data-oriented class constant lookup table indexed by case-sensitive [`PropKey`].
/// Eliminates runtime string hashing on `Class::CONST` / `self::CONST` / `$obj::CONST`.
pub(crate) struct ConstantTable {
    slots: Box<[Option<ConstantSlot>]>,
    hashed: bool,
}

impl ConstantTable {
    pub(crate) fn build(class: &Class) -> Self {
        if class.kind == ClassKind::Trait || class.constants.is_empty() {
            return Self {
                slots: Box::new([]),
                hashed: false,
            };
        }
        let keys = phera_bytecode::name_keys::prop_keys(class.constants.keys().map(String::as_str));
        if class.constants.len() <= 8 {
            let slots: Box<[Option<ConstantSlot>]> = class
                .constants
                .values()
                .zip(keys.iter().copied())
                .map(|(constant, key)| {
                    Some(ConstantSlot {
                        key: key.0,
                        owner: resolve_owner(class, &constant.declaring),
                        constant: constant.clone(),
                    })
                })
                .collect();
            Self {
                slots,
                hashed: false,
            }
        } else {
            let size = (class.constants.len() * 2).next_power_of_two().max(16);
            let mask = size - 1;
            let mut slots: Box<[Option<ConstantSlot>]> = (0..size).map(|_| None).collect();
            for (constant, key) in class.constants.values().zip(keys.iter().copied()) {
                let mut index = start(key.0, mask);
                while slots[index].is_some() {
                    index = (index + 1) & mask;
                }
                slots[index] = Some(ConstantSlot {
                    key: key.0,
                    owner: resolve_owner(class, &constant.declaring),
                    constant: constant.clone(),
                });
            }
            Self {
                slots,
                hashed: true,
            }
        }
    }

    #[inline(always)]
    pub(crate) fn get(&self, key: PropKey) -> Option<&ConstantSlot> {
        let slots = &self.slots;
        if !self.hashed {
            for slot in slots {
                let entry = slot.as_ref()?;
                if entry.key == key.0 {
                    return Some(entry);
                }
            }
            None
        } else {
            let mask = slots.len() - 1;
            let mut index = start(key.0, mask);
            loop {
                let entry = slots[index].as_ref()?;
                if entry.key == key.0 {
                    return Some(entry);
                }
                index = (index + 1) & mask;
            }
        }
    }
}

impl ConstantSlot {
    #[inline(always)]
    pub(crate) fn visible_to(&self, scope: Option<ClassIndex>, classes: &ClassTable) -> bool {
        match self.constant.visibility {
            Visibility::Public => true,
            Visibility::Private => {
                self.owner != UNRESOLVED_OWNER
                    && scope.is_some_and(|scope| scope.number() == self.owner)
            }
            Visibility::Protected => {
                if self.owner == UNRESOLVED_OWNER {
                    return false;
                }
                let Some(scope) = scope else {
                    return false;
                };
                if scope.number() == self.owner {
                    return true;
                }
                protected_visible(scope, self.owner, classes)
            }
        }
    }
}

/// Compact 16-byte `Copy` descriptor for a declared static property.
#[derive(Clone, Copy)]
pub(crate) struct PackedStaticProp {
    key: u32,
    owner: u32,
    slot: u32,
    flags: u32,
}

impl PackedStaticProp {
    const EMPTY: Self = Self {
        key: EMPTY_KEY,
        owner: UNRESOLVED_OWNER,
        slot: u32::MAX,
        flags: 0,
    };

    #[inline(always)]
    pub(crate) fn slot(self) -> super::class_state::StaticSlot {
        super::class_state::StaticSlot(self.slot)
    }

    /// Whether code executing in `scope` may access this static property.
    #[inline(always)]
    pub(crate) fn visible_to(self, scope: Option<ClassIndex>, classes: &ClassTable) -> bool {
        match (self.flags as u16) & VIS_MASK {
            VIS_PUBLIC => true,
            VIS_PRIVATE => {
                self.owner != UNRESOLVED_OWNER
                    && scope.is_some_and(|scope| scope.number() == self.owner)
            }
            VIS_PROTECTED => {
                if self.owner == UNRESOLVED_OWNER {
                    return false;
                }
                let Some(scope) = scope else {
                    return false;
                };
                if scope.number() == self.owner {
                    return true;
                }
                protected_visible(scope, self.owner, classes)
            }
            _ => false,
        }
    }
}

/// Flat data-oriented static property lookup table on [`Class`], indexed by
/// case-sensitive [`PropKey`]. Replaces per-instruction `CachedSite::Static`
/// caches with direct O(1) lookup that works on cold and polymorphic `static::`
/// sites with zero `RefCell` borrows.
pub(crate) struct StaticPropTable {
    slots: Box<[PackedStaticProp]>,
    hashed: bool,
}

impl StaticPropTable {
    pub(crate) fn build(class: &Class) -> Self {
        // Direct trait static access emits a deprecation every time, so keep it on the VM path.
        if class.kind == ClassKind::Trait || class.static_properties.is_empty() {
            return Self {
                slots: Box::new([]),
                hashed: false,
            };
        }
        let keys = phera_bytecode::name_keys::prop_keys(
            class
                .static_properties
                .iter()
                .map(|prop| prop.name.as_str()),
        );
        // PHP resolves static properties from most-derived declaration back to base (`iter().rev()`).
        let mut primary: Vec<PackedStaticProp> = Vec::with_capacity(class.static_properties.len());
        for (prop, key) in class
            .static_properties
            .iter()
            .zip(keys.iter().copied())
            .rev()
        {
            if primary.iter().any(|entry| entry.key == key.0) {
                continue;
            }
            let Some(super::class_state::StaticSlot(slot)) = prop.static_storage else {
                continue;
            };
            let flags = match prop.visibility {
                Visibility::Public => VIS_PUBLIC,
                Visibility::Protected => VIS_PROTECTED,
                Visibility::Private => VIS_PRIVATE,
            } as u32;
            primary.push(PackedStaticProp {
                key: key.0,
                owner: resolve_owner(class, &prop.declaring),
                slot,
                flags,
            });
        }
        if primary.len() <= 8 {
            Self {
                slots: primary.into_boxed_slice(),
                hashed: false,
            }
        } else {
            let size = (primary.len() * 2).next_power_of_two().max(16);
            let mask = size - 1;
            let mut table = vec![PackedStaticProp::EMPTY; size].into_boxed_slice();
            for entry in primary {
                let mut index = start(entry.key, mask);
                while table[index].key != EMPTY_KEY {
                    index = (index + 1) & mask;
                }
                table[index] = entry;
            }
            Self {
                slots: table,
                hashed: true,
            }
        }
    }

    #[inline(always)]
    pub(crate) fn get(&self, key: PropKey) -> Option<PackedStaticProp> {
        let slots = &self.slots;
        if !self.hashed {
            for &entry in slots {
                if entry.key == key.0 {
                    return Some(entry);
                }
            }
            None
        } else {
            let mask = slots.len() - 1;
            let mut index = start(key.0, mask);
            loop {
                let entry = slots[index];
                if entry.key == key.0 {
                    return Some(entry);
                }
                if entry.key == EMPTY_KEY {
                    return None;
                }
                index = (index + 1) & mask;
            }
        }
    }
}
