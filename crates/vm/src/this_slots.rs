//! Properties of `$this` whose slot a method knows when its class is linked.
//!
//! ```php
//! final class Money {
//!     private int $cents = 0;
//!     public function add(int $more): void {
//!         $this->cents = $this->cents + $more;   // the same slot, every call
//!     }
//! }
//! ```
//!
//! In a method of class `C`, `$this` is a `C` or a class that extends it.
//! A class's layout lists its parent's properties first, and a redeclared
//! property keeps its parent's place (`classes::class::property_layout`), so
//! a slot index of `C`'s layout means the same property in every subclass.
//! Two cases leave no doubt which property `$this->p` names there:
//! - `p` is private to `C`: `C`'s code always means `C`'s own `p`, and a
//!   subclass's `p` is another property with another key;
//! - `C` is final: `$this` is a `C`.
//!
//! For such a path the method's facts hold the slot, made the first time the
//! method reads or writes one, and the loop reads or writes the slot without
//! a property site. What can still differ per object is checked on each
//! access: the object may be lazy, the slot may be empty (unset, or not
//! initialized yet) or hold a PHP reference, or the object's properties may
//! have left the layout. Each of those goes to the property site and the VM.
//! A property with hooks, a name a static property also has (PHP's notice),
//! a closure (its `$this` can be any object) and a class whose properties
//! native code supplies get no slot here.

use phera_bytecode::{PathBase, Step, TypeMask, Visibility};

use crate::classes::property_facts::PropertyFacts;
use crate::classes::{Class, ClassTable};
use crate::code_table::{CodeTable, FunctionIndex};

/// What a method knows about one of its paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThisSlot {
    /// Not a property of `$this` whose slot is certain.
    Unknown,
    /// `$this->p` lives in this slot of every object the method can see.
    Certain { slot: u32, write: SlotWrite },
}

/// Whether a write of `$this->p` may replace the slot's value directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotWrite {
    /// The VM writes it: readonly, `private(set)` and the like.
    General,
    /// Any value.
    Untyped,
    /// A value whose own type is in the declared type, unconverted.
    Typed(TypeMask),
}

impl SlotWrite {
    /// Whether `value` may be stored as it is.
    pub(crate) fn accepts(self, value: &phera_value::Value) -> bool {
        match self {
            SlotWrite::General => false,
            SlotWrite::Untyped => true,
            SlotWrite::Typed(mask) => mask.intersects(crate::types::own_type(value)),
        }
    }
}

/// The slot of each of the method's paths, by path number.
pub(crate) fn this_slots(
    code: &CodeTable,
    classes: &ClassTable,
    function: FunctionIndex,
) -> Box<[ThisSlot]> {
    let body = code.function(function);
    let mut slots = vec![ThisSlot::Unknown; body.paths.len()];
    let Some(class) = code.facts(function).class.map(|index| classes.class(index)) else {
        return slots.into_boxed_slice();
    };
    if class.kind == phera_bytecode::ClassKind::Trait
        || class.has_native_layout
        || class.property_facts().has(PropertyFacts::HOOKS)
    {
        return slots.into_boxed_slice();
    }
    let names = &code.unit(function).names;
    for (slot, path) in slots.iter_mut().zip(body.paths.iter()) {
        if let (PathBase::This { .. }, [Step::Property(name)]) = (path.base, path.steps.as_slice())
            && let Some(name) = names.get(name.index())
        {
            *slot = certain_slot(class, name);
        }
    }
    slots.into_boxed_slice()
}

/// The slot `$this->name` has in every object a method of `class` sees, if
/// there is no doubt which property it is (see the module).
fn certain_slot(class: &Class, name: &str) -> ThisSlot {
    if class
        .static_properties
        .iter()
        .any(|property| property.name == name)
    {
        return ThisSlot::Unknown;
    }
    let own_private = class.defaults.iter().find(|property| {
        property.name == name
            && property.visibility == Visibility::Private
            && property.declaring == class.name
    });
    let property = own_private.or_else(|| {
        // In a final class, the property PHP resolves from inside it: the
        // last one of the name that the class can see.
        class.is_final.then(|| {
            class.defaults.iter().rev().find(|property| {
                property.name == name
                    && (property.visibility != Visibility::Private
                        || property.declaring == class.name)
            })
        })?
    });
    let Some(property) = property else {
        return ThisSlot::Unknown;
    };
    if property.hooks.is_some() {
        return ThisSlot::Unknown;
    }
    let Some(slot) = class
        .layout
        .as_ref()
        .and_then(|layout| layout.slot(&property.key))
        .and_then(|slot| u32::try_from(slot).ok())
    else {
        return ThisSlot::Unknown;
    };
    let write = if property.is_readonly || property.set_visibility.is_some() {
        SlotWrite::General
    } else {
        match &property.ty {
            None => SlotWrite::Untyped,
            Some(ty) => SlotWrite::Typed(ty.mask),
        }
    };
    ThisSlot::Certain { slot, write }
}
