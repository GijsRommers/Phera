//! Flat data-oriented property lookup table on [`Class`].
//!
//! Maps a process-interned case-sensitive [`PropKey`] directly to a compact
//! 16-byte [`PackedProp`] descriptor (slot index, visibility/hook/readonly/type
//! bitset, declaring class index) with zero per-instruction heap caches, zero
//! `RefCell` borrows, and zero string comparisons at runtime.

use phera_bytecode::{ClassKind, PropKey, TypeMask, Visibility};

use super::class::Class;
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

/// Flat property lookup table built once per [`Class`] and shared across forks.
pub(crate) struct PropertyTable {
    slots: Box<[PackedProp]>,
    shadows: Box<[PackedProp]>,
    table_flags: u8,
}

impl PropertyTable {
    pub(crate) fn build(class: &Class) -> Self {
        let facts = class.property_facts();
        let mut table_flags = 0u8;
        if class.property_access.is_some()
            || phera_value::ClassHandle::has_native_properties(class)
        {
            table_flags |= TABLE_HAS_NATIVE_PROPS;
        }
        if facts.has(PropertyFacts::MAGIC_GET) {
            table_flags |= TABLE_HAS_MAGIC_GET;
        }
        if facts.has(PropertyFacts::MAGIC_ISSET) {
            table_flags |= TABLE_HAS_MAGIC_ISSET;
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
                table_flags,
            };
        }

        let keys = phera_bytecode::name_keys::prop_keys(
            class.defaults.iter().map(|prop| prop.name.as_str()),
        );
        let mut primary: Vec<PackedProp> = Vec::with_capacity(class.defaults.len());
        let mut shadows: Vec<PackedProp> = Vec::new();

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

        if primary.len() <= 8 {
            Self {
                slots: primary.into_boxed_slice(),
                shadows: shadows.into_boxed_slice(),
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
                table_flags,
            }
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
