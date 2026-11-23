//! An object's properties: the declared ones in slots, in the class's
//! order, and the others in a table of their own.
//!
//! ```php
//! class P { public $a = 1; public $b = 2; public $c = 3; }
//! $o = new P;              // slots [1, 2, 3], no other properties
//! unset($o->a);            // slots [-, 2, 3]
//! $o->x = 9;               // slots [-, 2, 3], others {x: 9}
//! $o->a = 1;               // slots [1, 2, 3], others {x: 9}
//! var_dump(array_keys(get_object_vars($o)));  // a b c x, as in PHP
//! ```
//!
//! The class's [`PropertyLayout`] holds the declared properties' keys once,
//! in declaration order, parents first: the plain name of a public
//! property, `\0*\0name` for a protected one and `\0Class\0name` for a
//! private one, as PHP stores them. An object holds one slot per key, and
//! `None` where the property is unset or not initialized yet. A declared
//! property keeps its place when it is unset and set again, which is PHP's
//! order. Properties the class does not declare (and every property of a
//! class without a layout, such as `stdClass`) are kept in insertion order
//! in a [`PhpArray`] after the slots.
//!
//! Code that knows a property's slot (a property site, a
//! `ReflectionProperty`) reads and writes it by number. Everything else
//! uses the key, with the methods `PhpArray` has, so that code which
//! looked up a property in the old table reads the same here.

use std::rc::Rc;

use crate::array::{ArrayKey, PhpArray};
use crate::value::Value;

/// A class's declared instance properties, in the order PHP lists them.
#[derive(Debug, PartialEq)]
pub struct PropertyLayout {
    keys: Box<[ArrayKey]>,
}

impl PropertyLayout {
    pub fn new(keys: impl IntoIterator<Item = ArrayKey>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
        }
    }

    /// The slot of the property stored under `key`, if the class declares it.
    #[inline]
    pub fn slot(&self, key: &ArrayKey) -> Option<usize> {
        self.keys.iter().position(|declared| declared == key)
    }

    pub fn keys(&self) -> &[ArrayKey] {
        &self.keys
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Up to four declared properties live inline inside `Properties` so small
/// objects (DTOs, counters, enums, iterators, proxies) need no second heap
/// allocation for their slot slice.
const INLINE_SLOTS: usize = 4;

#[derive(Clone, Default)]
enum PropertySlots {
    #[default]
    Empty,
    Inline {
        len: u8,
        values: [Option<Value>; INLINE_SLOTS],
    },
    Heap(Box<[Option<Value>]>),
}

impl PropertySlots {
    #[inline]
    fn from_len(len: usize) -> Self {
        if len == 0 {
            Self::Empty
        } else if len <= INLINE_SLOTS {
            Self::Inline {
                len: len as u8,
                values: [const { None }; INLINE_SLOTS],
            }
        } else {
            Self::Heap(vec![None; len].into_boxed_slice())
        }
    }

    fn from_boxed(slots: Box<[Option<Value>]>) -> Self {
        if slots.is_empty() {
            Self::Empty
        } else if slots.len() <= INLINE_SLOTS {
            let len = slots.len() as u8;
            let mut values = [const { None }; INLINE_SLOTS];
            for (dst, src) in values.iter_mut().zip(slots.into_vec()) {
                *dst = src;
            }
            Self::Inline { len, values }
        } else {
            Self::Heap(slots)
        }
    }

    #[inline(always)]
    fn as_slice(&self) -> &[Option<Value>] {
        match self {
            Self::Empty => &[],
            Self::Inline { len, values } => &values[..usize::from(*len)],
            Self::Heap(slots) => slots,
        }
    }

    #[inline(always)]
    fn as_mut_slice(&mut self) -> &mut [Option<Value>] {
        match self {
            Self::Empty => &mut [],
            Self::Inline { len, values } => &mut values[..usize::from(*len)],
            Self::Heap(slots) => slots,
        }
    }

    #[inline]
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Empty | Self::Inline { .. } => 0,
            Self::Heap(slots) => slots.len() * std::mem::size_of::<Option<Value>>(),
        }
    }
}

/// The properties of one object.
#[derive(Clone, Default)]
pub struct Properties {
    /// The class's layout; `None` for a class without declared properties.
    layout: Option<Rc<PropertyLayout>>,
    /// One per key of the layout (inline for up to four declared properties).
    slots: PropertySlots,
    /// Properties the class does not declare, in insertion order; made on
    /// the first one, since most objects have none.
    others: Option<Box<PhpArray>>,
}

impl Properties {
    /// Properties of a class with this layout, the slots given in layout
    /// order (`None` for a property without a value yet).
    pub fn with_layout(layout: Rc<PropertyLayout>, slots: Box<[Option<Value>]>) -> Self {
        debug_assert_eq!(layout.len(), slots.len());
        Self {
            layout: Some(layout),
            slots: PropertySlots::from_boxed(slots),
            others: None,
        }
    }

    /// No properties yet, for an object of `class`: an empty slot for each
    /// declared property when the class keeps them in slots.
    pub fn for_class(class: &dyn crate::object::ClassHandle) -> Self {
        match class.property_layout() {
            Some(layout) => {
                let slots = PropertySlots::from_len(layout.len());
                Self {
                    layout: Some(layout),
                    slots,
                    others: None,
                }
            }
            None => Self::default(),
        }
    }

    /// Properties of a class without a layout: all of them by key.
    pub fn from_array(others: PhpArray) -> Self {
        Self {
            layout: None,
            slots: PropertySlots::Empty,
            others: Some(Box::new(others)),
        }
    }

    pub fn layout(&self) -> Option<&Rc<PropertyLayout>> {
        self.layout.as_ref()
    }

    #[inline]
    fn slot_of(&self, key: &ArrayKey) -> Option<usize> {
        self.layout.as_ref()?.slot(key)
    }

    fn others_or_new(&mut self) -> &mut PhpArray {
        self.others.get_or_insert_with(Box::default)
    }

    // ----- By slot number -------------------------------------------------

    /// The value in slot `slot`, if it has one.
    #[inline]
    pub fn slot(&self, slot: usize) -> Option<&Value> {
        self.slots.as_slice().get(slot)?.as_ref()
    }

    #[inline]
    pub fn slot_mut(&mut self, slot: usize) -> Option<&mut Value> {
        self.slots.as_mut_slice().get_mut(slot)?.as_mut()
    }

    /// Puts `value` in slot `slot` and gives what it held; `None` (with
    /// nothing written) when there is no such slot.
    #[inline]
    pub fn set_slot(&mut self, slot: usize, value: Value) -> Option<Option<Value>> {
        let held = self.slots.as_mut_slice().get_mut(slot)?;
        Some(held.replace(value))
    }

    /// Empties slot `slot` and gives what it held.
    #[inline]
    pub fn clear_slot(&mut self, slot: usize) -> Option<Value> {
        self.slots.as_mut_slice().get_mut(slot)?.take()
    }

    /// The slots, in layout order.
    pub fn slots(&self) -> &[Option<Value>] {
        self.slots.as_slice()
    }

    /// The slots, writable, in layout order.
    pub(crate) fn slots_mut(&mut self) -> &mut [Option<Value>] {
        self.slots.as_mut_slice()
    }

    /// The slot of `key`, using `hint` (a slot a site or a
    /// `ReflectionProperty` remembered) when the layout has `key` there.
    #[inline]
    pub fn slot_for(&self, key: &ArrayKey, hint: Option<usize>) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        match hint {
            Some(slot) if layout.keys.get(slot) == Some(key) => Some(slot),
            _ => layout.slot(key),
        }
    }

    /// [`Properties::get`], with a remembered slot.
    #[inline]
    pub fn get_at(&self, key: &ArrayKey, hint: Option<usize>) -> Option<&Value> {
        match self.slot_for(key, hint) {
            Some(slot) => self.slot(slot),
            None => self.others.as_deref()?.get(key),
        }
    }

    /// [`Properties::get_mut`], with a remembered slot.
    #[inline]
    pub fn get_mut_at(&mut self, key: &ArrayKey, hint: Option<usize>) -> Option<&mut Value> {
        match self.slot_for(key, hint) {
            Some(slot) => self.slot_mut(slot),
            None => self.others.as_deref_mut()?.get_mut(key),
        }
    }

    /// [`Properties::insert`], with a remembered slot.
    #[inline]
    pub fn insert_at(&mut self, key: ArrayKey, hint: Option<usize>, value: Value) -> Option<Value> {
        match self.slot_for(&key, hint) {
            Some(slot) => self.set_slot(slot, value).flatten(),
            None => self.others_or_new().insert(key, value),
        }
    }

    // ----- By key, as `PhpArray` ------------------------------------------

    pub fn get(&self, key: &ArrayKey) -> Option<&Value> {
        self.get_at(key, None)
    }

    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<&mut Value> {
        self.get_mut_at(key, None)
    }

    pub fn contains_key(&self, key: &ArrayKey) -> bool {
        self.get(key).is_some()
    }

    /// Sets a property, creating it if needed, and returns what it held.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        self.insert_at(key, None, value)
    }

    /// Removes a property and returns what it held. A declared property's
    /// slot stays, empty, so that setting it again keeps its place.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        match self.slot_of(key) {
            Some(slot) => self.clear_slot(slot),
            None => self.others.as_deref_mut()?.remove(key),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.as_slice().iter().flatten().count()
            + self.others.as_deref().map_or(0, PhpArray::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The properties in PHP's order: the declared ones that have a value,
    /// then the others.
    pub fn iter(&self) -> impl Iterator<Item = (ArrayKey, &Value)> {
        let keys = self.layout.as_ref().map_or(&[][..], |layout| layout.keys());
        keys.iter()
            .zip(self.slots.as_slice().iter())
            .filter_map(|(key, value)| Some((key.clone(), value.as_ref()?)))
            .chain(self.others.as_deref().into_iter().flat_map(PhpArray::iter))
    }

    /// The values in PHP's order.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.slots.as_slice().iter().flatten().chain(
            self.others
                .as_deref()
                .into_iter()
                .flat_map(PhpArray::values),
        )
    }

    /// The memory the properties hold beyond the object itself.
    pub fn heap_bytes(&self) -> usize {
        self.slots.heap_bytes()
            + self.others.as_deref().map_or(0, |others| {
                std::mem::size_of::<PhpArray>() + others.heap_bytes()
            })
    }

    /// Every value, writable, in PHP's order.
    pub fn for_each_mut(&mut self, mut visit: impl FnMut(&ArrayKey, &mut Value)) {
        if let Some(layout) = &self.layout {
            for (key, value) in layout
                .keys()
                .iter()
                .zip(self.slots.as_mut_slice().iter_mut())
            {
                if let Some(value) = value {
                    visit(key, value);
                }
            }
        }
        if let Some(others) = self.others.as_deref_mut() {
            for (key, value) in others.iter_mut() {
                visit(&key, value);
            }
        }
    }

    /// The properties as one array in PHP's order: `get_object_vars`,
    /// `var_dump`.
    pub fn to_array(&self) -> PhpArray {
        self.array_of(false)
    }

    /// The properties as `(array) $object` makes them: in PHP's order, with
    /// the name of an undeclared property that reads as an integer (`5`,
    /// from `(object) [5 => 'five']` or `$object->{'5'}`) as an integer key.
    pub fn to_cast_array(&self) -> PhpArray {
        self.array_of(true)
    }

    fn array_of(&self, integer_names: bool) -> PhpArray {
        if self.layout.is_none() {
            let others = self.others.as_deref().cloned().unwrap_or_default();
            return if integer_names {
                Self::with_integer_names(others)
            } else {
                others
            };
        }
        // A declared key and an undeclared one never coincide, so the keys
        // are distinct and the array is built in one go, at its size.
        let capacity =
            self.slots.as_slice().len() + self.others.as_deref().map_or(0, PhpArray::len);
        let array = PhpArray::build_distinct(capacity, |entries| {
            if let Some(layout) = &self.layout {
                for (key, slot) in layout.keys().iter().zip(self.slots.as_slice().iter()) {
                    if let Some(value) = slot {
                        entries.push(key.clone(), copy_element(value));
                    }
                }
            }
            if let Some(others) = self.others.as_deref() {
                for (key, value) in others.iter() {
                    entries.push(key, copy_element(value));
                }
            }
        });
        match self.others {
            Some(_) if integer_names => Self::with_integer_names(array),
            _ => array,
        }
    }

    /// `array` with each string key that reads as an integer made one. Two
    /// keys cannot meet: `"5"` and `5` are not both properties, and `"05"`
    /// stays a string.
    fn with_integer_names(array: PhpArray) -> PhpArray {
        let numeric = array.keys().any(|key| match &key {
            crate::array::ArrayKey::String(name) => name.numeric_key().is_some(),
            crate::array::ArrayKey::Int(_) => false,
        });
        if !numeric {
            return array;
        }
        let entries = array.iter().map(|(key, value)| {
            let key = match key {
                crate::array::ArrayKey::String(name) => crate::array::ArrayKey::from_string(name),
                key => key,
            };
            Ok::<_, std::convert::Infallible>((key, value.clone()))
        });
        match PhpArray::try_from_distinct_with_capacity(entries, array.len()) {
            Ok(array) => array,
            Err(never) => match never {},
        }
    }

    /// The properties that are not declared, in insertion order, if any.
    pub fn others(&self) -> Option<&PhpArray> {
        self.others.as_deref()
    }

    /// The same, writable.
    pub fn others_mut(&mut self) -> Option<&mut PhpArray> {
        self.others.as_deref_mut()
    }

    /// Replaces every property with those of `array`, by key: declared
    /// keys go to their slots, the rest to the other properties.
    ///
    /// A table whose order the slots cannot keep (`ArrayObject::ksort` on
    /// an object reorders its properties, declared ones included) leaves
    /// the layout: every property is then kept by key, in the given order,
    /// and slot access finds nothing, so sites and `ReflectionProperty`
    /// take the general path for this object.
    pub fn replace_all(&mut self, array: PhpArray) {
        let Some(layout) = self.layout.clone() else {
            self.others = (!array.is_empty()).then(|| Box::new(array));
            return;
        };
        if !in_layout_order(&layout, &array) {
            self.layout = None;
            self.slots = PropertySlots::Empty;
            self.others = (!array.is_empty()).then(|| Box::new(array));
            return;
        }
        self.slots
            .as_mut_slice()
            .iter_mut()
            .for_each(|slot| *slot = None);
        self.others = None;
        for (key, value) in array.iter() {
            match layout
                .slot(&key)
                .and_then(|slot| self.slots.as_mut_slice().get_mut(slot))
            {
                Some(slot) => *slot = Some(value.clone()),
                None => {
                    self.others_or_new().insert(key, value.clone());
                }
            }
        }
    }

    /// Moves every value out, for a release that must not recurse. They go
    /// in last first, so that `pop` frees them in PHP's order: the
    /// declared properties first to last, then the others.
    pub(crate) fn take_values(mut self, work: &mut Vec<Value>) {
        if let Some(mut others) = self.others.take() {
            others.take_values(work);
        }
        match std::mem::take(&mut self.slots) {
            PropertySlots::Empty => {}
            PropertySlots::Inline { len, values } => {
                work.extend(values.into_iter().take(usize::from(len)).rev().flatten());
            }
            PropertySlots::Heap(slots) => {
                work.extend(slots.into_vec().into_iter().rev().flatten());
            }
        }
    }
}

fn may_recurse_on_drop(slot: &Option<Value>) -> bool {
    matches!(
        slot,
        Some(Value::Array(_) | Value::Object(_) | Value::Reference(_))
    )
}

/// Whether `array` lists the declared keys of `layout` in layout order and
/// every other key after them: the only order the slots can keep.
fn in_layout_order(layout: &PropertyLayout, array: &PhpArray) -> bool {
    let mut last = None;
    let mut others = false;
    for (key, _) in array.iter() {
        match layout.slot(&key) {
            Some(slot) if !others && last.is_none_or(|last| slot > last) => last = Some(slot),
            Some(_) => return false,
            None => others = true,
        }
    }
    true
}

/// A property's value as a copy of the properties holds it: a PHP
/// reference that only the property holds is copied as its value, as an
/// array's copy does (`PhpArray`'s `Clone`).
fn copy_element(value: &Value) -> Value {
    if let Value::Reference(reference) = value
        && Rc::strong_count(reference) == 1
        && let Ok(inner) = reference.try_borrow()
    {
        return inner.clone();
    }
    value.clone()
}

impl Drop for Properties {
    /// Releases the slots as an array releases its values: a nested array,
    /// object or reference goes through a work list instead of recursion,
    /// so that a list of 300,000 linked objects is freed without
    /// overflowing the stack. The other properties are a `PhpArray`, which
    /// does the same itself.
    fn drop(&mut self) {
        match std::mem::take(&mut self.slots) {
            PropertySlots::Empty => {}
            PropertySlots::Inline { len, values } => {
                let len = usize::from(len);
                if values[..len].iter().any(may_recurse_on_drop) {
                    PhpArray::release_entries(values.into_iter().take(len).flatten());
                }
            }
            PropertySlots::Heap(slots) => {
                if slots.iter().any(may_recurse_on_drop) {
                    PhpArray::release_entries(slots.into_vec().into_iter().flatten());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table of properties the class does not declare: `(array) $object`
    /// reads the names of integers as integers, `get_object_vars` and
    /// `var_dump` as they are.
    #[test]
    fn a_cast_makes_integer_keys_of_names_that_read_as_integers() {
        let mut others = PhpArray::new();
        others.insert(ArrayKey::str("a"), Value::Int(1));
        others.insert(ArrayKey::str("5"), Value::Int(2));
        others.insert(ArrayKey::str("05"), Value::Int(3));
        let properties = Properties::from_array(others);
        assert_eq!(
            properties.to_array().keys().collect::<Vec<_>>(),
            vec![ArrayKey::str("a"), ArrayKey::str("5"), ArrayKey::str("05")]
        );
        assert_eq!(
            properties.to_cast_array().keys().collect::<Vec<_>>(),
            vec![ArrayKey::str("a"), ArrayKey::Int(5), ArrayKey::str("05")]
        );
    }

    /// Declared properties come first, in layout order, those without a
    /// value left out, then the undeclared ones; a lone reference is copied.
    #[test]
    fn a_cast_lists_declared_properties_then_the_others() {
        let layout = Rc::new(PropertyLayout::new([
            ArrayKey::str("\0*\0hidden"),
            ArrayKey::str("unset"),
            ArrayKey::str("shown"),
        ]));
        let mut properties = Properties::with_layout(
            layout,
            vec![
                Some(Value::Int(1)),
                None,
                Some(Value::Reference(Rc::new(std::cell::RefCell::new(
                    Value::Int(2),
                )))),
            ]
            .into_boxed_slice(),
        );
        properties.insert(ArrayKey::str("dynamic"), Value::Int(3));
        let array = properties.to_cast_array();
        assert_eq!(
            array.keys().collect::<Vec<_>>(),
            vec![
                ArrayKey::str("\0*\0hidden"),
                ArrayKey::str("shown"),
                ArrayKey::str("dynamic")
            ]
        );
        assert!(matches!(
            array.get(&ArrayKey::str("shown")),
            Some(Value::Int(2))
        ));
    }
}
