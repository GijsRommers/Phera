//! A class's methods by interned name, in a flat 16-byte-per-slot SoA table.
//!
//! ```php
//! $point->add(1, 2);   // the compiler wrote "add" as a name of the file,
//!                      // the code table gave it the number 42,
//!                      // and Point's table maps 42 to Point::add
//! ```
//!
//! Every slot is 16 bytes and `Copy` (4 slots per cache line), packing the
//! interned `NameKey`, declaring `ClassIndex`, `FunctionIndex` (for PHP
//! methods), visibility, kind, and static/abstract flags. PHP method calls
//! read the slot in registers with zero `Rc<Method>` dereference, zero
//! `Weak::upgrade()`, and zero string comparison. Only non-PHP methods
//! (native, enum, closure) store side metadata in `extras`.

use std::rc::{Rc, Weak};

use phera_bytecode::Visibility;

use super::{Class, ClassIndex, Method, MethodBody};
use crate::code_table::{FunctionIndex, NameKey};

const EMPTY_KEY: u32 = u32::MAX;

const VIS_MASK: u16 = 0b11;
const KIND_SHIFT: u16 = 2;
const KIND_MASK: u16 = 0b11;
const FLAG_STATIC: u16 = 1 << 4;
const FLAG_ABSTRACT: u16 = 1 << 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    User = 0,
    Native = 1,
    Enum = 2,
    Closure = 3,
}

/// Compact 16-byte `Copy` method descriptor stored directly in [`MethodTable`].
#[derive(Clone, Copy)]
pub(crate) struct PackedMethod {
    key: u32,
    /// Declaring class index (`ClassIndex::INVALID` if unavailable).
    pub(crate) declaring: ClassIndex,
    /// `FunctionIndex.0` when `kind() == MethodKind::User`, otherwise an index
    /// into [`MethodTable::extras`].
    payload: u32,
    flags: u16,
    _pad: u16,
}

const _: () = assert!(std::mem::size_of::<PackedMethod>() == 16);

impl PackedMethod {
    const EMPTY: Self = Self {
        key: EMPTY_KEY,
        declaring: ClassIndex::INVALID,
        payload: 0,
        flags: 0,
        _pad: 0,
    };

    #[inline(always)]
    pub(crate) fn is_empty(self) -> bool {
        self.key == EMPTY_KEY
    }

    #[inline(always)]
    pub(crate) fn visibility(self) -> Visibility {
        match self.flags & VIS_MASK {
            0 => Visibility::Public,
            1 => Visibility::Protected,
            _ => Visibility::Private,
        }
    }

    #[inline(always)]
    pub(crate) fn kind(self) -> MethodKind {
        match (self.flags >> KIND_SHIFT) & KIND_MASK {
            0 => MethodKind::User,
            1 => MethodKind::Native,
            2 => MethodKind::Enum,
            _ => MethodKind::Closure,
        }
    }

    #[inline(always)]
    pub(crate) fn is_static(self) -> bool {
        self.flags & FLAG_STATIC != 0
    }

    #[inline(always)]
    pub(crate) fn is_abstract(self) -> bool {
        self.flags & FLAG_ABSTRACT != 0
    }

    /// The PHP body function index when `self.kind() == MethodKind::User`.
    #[inline(always)]
    pub(crate) fn user_function(self) -> Option<FunctionIndex> {
        (self.kind() == MethodKind::User).then_some(FunctionIndex(self.payload))
    }

    /// Index into `MethodTable::extras` for non-PHP methods.
    #[inline(always)]
    fn extra_index(self) -> usize {
        self.payload as usize
    }
}

/// Side entry kept only for non-PHP methods (native, enum, closure).
#[derive(Clone)]
pub(crate) struct MethodExtra {
    pub(crate) method: Rc<Method>,
    pub(crate) scope: Weak<Class>,
}

/// The methods of one class by name key.
#[derive(Clone)]
pub(crate) struct MethodTable {
    /// Power-of-two open-addressed table of 16-byte `Copy` descriptors.
    slots: Box<[PackedMethod]>,
    /// Side metadata for native/enum/closure methods (empty for pure PHP classes).
    extras: Box<[MethodExtra]>,
    /// Whether any method in the table is private. A call made from inside
    /// a class must first ask whether that class has a private method of
    /// the name (PHP calls it then, not an inherited one); for a class with
    /// none, which is most, this answers without a lookup.
    has_private: bool,
}

impl MethodTable {
    /// Number every method of `class`. `scope_of` finds the class that
    /// declares a method.
    pub(crate) fn build(class: &Class, scope_of: impl Fn(&Method) -> Option<Rc<Class>>) -> Self {
        // A direct trait call emits a deprecation every time. Composed
        // methods have their own table on the consuming class.
        if class.kind == phera_bytecode::ClassKind::Trait || class.methods.is_empty() {
            return Self {
                slots: Box::new([PackedMethod::EMPTY]),
                extras: Box::default(),
                has_private: false,
            };
        }
        let size = (class.methods.len() * 2).next_power_of_two().max(4);
        let mut slots: Box<[PackedMethod]> = vec![PackedMethod::EMPTY; size].into_boxed_slice();
        let mut extras: Vec<MethodExtra> = Vec::new();
        let mut has_private = false;
        let keys = phera_bytecode::name_keys::keys(class.methods.keys().map(String::as_str));
        for ((_, method), key) in class.methods.iter().zip(keys.iter().copied()) {
            let scope = scope_of(method);
            let declaring = scope
                .as_ref()
                .map_or(ClassIndex::INVALID, |scope| scope.index);
            let (kind, payload) = match &method.body {
                MethodBody::User(function) => (MethodKind::User, function.0),
                MethodBody::Native(_) => {
                    let idx = extras.len() as u32;
                    extras.push(MethodExtra {
                        method: method.clone(),
                        scope: scope.map_or_else(Weak::new, |scope| Rc::downgrade(&scope)),
                    });
                    (MethodKind::Native, idx)
                }
                MethodBody::Enum(_) => {
                    let idx = extras.len() as u32;
                    extras.push(MethodExtra {
                        method: method.clone(),
                        scope: scope.map_or_else(Weak::new, |scope| Rc::downgrade(&scope)),
                    });
                    (MethodKind::Enum, idx)
                }
                MethodBody::Closure(_) => {
                    let idx = extras.len() as u32;
                    extras.push(MethodExtra {
                        method: method.clone(),
                        scope: scope.map_or_else(Weak::new, |scope| Rc::downgrade(&scope)),
                    });
                    (MethodKind::Closure, idx)
                }
            };
            let vis_bits: u16 = match method.visibility {
                Visibility::Public => 0,
                Visibility::Protected => 1,
                Visibility::Private => 2,
            };
            let mut flags = vis_bits | ((kind as u16) << KIND_SHIFT);
            if method.is_static {
                flags |= FLAG_STATIC;
            }
            if method.is_abstract {
                flags |= FLAG_ABSTRACT;
            }
            has_private |= method.visibility == Visibility::Private;
            let mut index = start(key, size);
            while !slots[index].is_empty() {
                index = (index + 1) & (size - 1);
            }
            slots[index] = PackedMethod {
                key: key.0,
                declaring,
                payload,
                flags,
                _pad: 0,
            };
        }
        Self {
            slots,
            extras: extras.into_boxed_slice(),
            has_private,
        }
    }

    /// The private method with this name key, if the class has one.
    #[inline(always)]
    pub(crate) fn private(&self, key: NameKey) -> Option<PackedMethod> {
        if !self.has_private {
            return None;
        }
        self.get(key)
            .filter(|packed| packed.visibility() == Visibility::Private)
    }

    /// The packed method descriptor for `key`, in 2 registers with zero indirection.
    #[inline(always)]
    pub(crate) fn get(&self, key: NameKey) -> Option<PackedMethod> {
        let mask = self.slots.len() - 1;
        let mut index = start(key, self.slots.len());
        loop {
            let slot = self.slots[index];
            if slot.key == key.0 {
                return Some(slot);
            }
            if slot.is_empty() {
                return None;
            }
            index = (index + 1) & mask;
        }
    }

    /// Side metadata (`Rc<Method>` and declaring `Weak<Class>`) for a non-PHP method.
    #[inline]
    pub(crate) fn extra(&self, packed: PackedMethod) -> Option<&MethodExtra> {
        (packed.kind() != MethodKind::User)
            .then(|| self.extras.get(packed.extra_index()))
            .flatten()
    }
}

/// Where the search for `key` starts in a table of `size` slots.
#[inline(always)]
fn start(key: NameKey, size: usize) -> usize {
    (key.0.wrapping_mul(0x9E37_79B9) >> 16) as usize & (size - 1)
}
