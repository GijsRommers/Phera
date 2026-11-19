//! `a:2:{i:0;i:1;i:1;R:2;}` binds both entries to one PHP reference cell.
//!
//! PHP numbers locations, including overwritten keys. Values and their locations
//! are stored separately so overwriting a key can leave earlier aliases alive.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use phera_extension_api::{ArrayKey, Context, NativeError, Object, PhpArray, Value};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct Place(usize);

#[derive(Clone, Copy)]
pub(super) struct Content(usize);

#[derive(Clone, Copy)]
enum Binding {
    Direct(Content),
    Reference(Content),
}

impl Binding {
    fn content(self) -> Content {
        match self {
            Self::Direct(content) | Self::Reference(content) => content,
        }
    }
}

type Reference = Rc<RefCell<Value>>;

struct Stored {
    value: Value,
    cell: Option<Reference>,
    pending_array: bool,
}

#[derive(Default)]
pub(super) struct Members(PhpArray);

impl Members {
    pub(super) fn contains_key(&self, key: &ArrayKey) -> bool {
        self.0.contains_key(key)
    }
}

#[derive(Default)]
pub(super) struct Graph {
    places: Vec<Option<Binding>>,
    values: Vec<Stored>,
    slots: Vec<Place>,
    arrays: Vec<(Content, Members)>,
    objects: Vec<(Rc<Object>, Members)>,
    cyclic_references: Vec<Reference>,
}

impl Graph {
    pub(super) fn new_place(&mut self) -> Place {
        let place = Place(self.places.len());
        self.places.push(None);
        place
    }

    pub(super) fn member(&mut self, members: &mut Members, key: ArrayKey) -> Place {
        if let Some(Value::Int(place)) = members.0.get(&key) {
            return Place(*place as usize);
        }
        let place = self.new_place();
        members.0.insert(key, Value::Int(place.0 as i64));
        place
    }

    pub(super) fn enter(&mut self, place: Place, takes_slot: bool) {
        self.places[place.0] = None;
        if takes_slot {
            self.slots.push(place);
        }
    }

    pub(super) fn value(&mut self, place: Place, value: Value) {
        self.store(place, value, false);
    }

    pub(super) fn start_array(&mut self, place: Place) -> Content {
        self.store(place, Value::Null, true)
    }

    pub(super) fn finish_array(&mut self, content: Content, members: Members) {
        self.values[content.0].pending_array = false;
        self.arrays.push((content, members));
    }

    pub(super) fn finish_object(&mut self, object: Rc<Object>, members: Members) {
        self.objects.push((object, members));
    }

    fn store(&mut self, place: Place, value: Value, pending_array: bool) -> Content {
        let content = Content(self.values.len());
        self.values.push(Stored {
            value,
            cell: None,
            pending_array,
        });
        self.places[place.0] = Some(Binding::Direct(content));
        content
    }

    pub(super) fn reference(&mut self, place: Place, slot: usize, by_ref: bool) -> Option<()> {
        let target = *self.slots.get(slot.checked_sub(1)?)?;
        if target == place {
            return None;
        }
        let binding = self.places[target.0]?;
        let content = binding.content();
        let stored = &mut self.values[content.0];
        if by_ref {
            if stored.cell.is_none() {
                let cell = Rc::new(RefCell::new(stored.value.clone()));
                if stored.pending_array {
                    self.cyclic_references.push(cell.clone());
                }
                stored.cell = Some(cell);
            }
            let promoted = Some(Binding::Reference(content));
            self.places[target.0] = promoted;
            self.places[place.0] = promoted;
        } else {
            let Value::Object(object) = &stored.value else {
                return None;
            };
            let value = Value::Object(object.clone());
            self.value(place, value);
        }
        Some(())
    }

    pub(super) fn get(&self, place: Place) -> Value {
        match self.places[place.0] {
            Some(Binding::Direct(content)) => self.values[content.0].value.clone(),
            Some(Binding::Reference(content)) => self.values[content.0].cell.as_ref().map_or_else(
                || self.values[content.0].value.clone(),
                |cell| Value::Reference(cell.clone()),
            ),
            None => Value::Null,
        }
    }

    pub(super) fn array(&self, members: Members) -> PhpArray {
        let mut entries = members.0;
        for (_, value) in entries.iter_mut() {
            if let Value::Int(place) = value {
                *value = self.get(Place(*place as usize));
            }
        }
        entries
    }

    pub(super) fn materialize(&mut self, context: &mut dyn Context) -> Result<(), NativeError> {
        for (content, members) in std::mem::take(&mut self.arrays) {
            let array = Value::Array(Rc::new(self.array(members)));
            let stored = &mut self.values[content.0];
            if let Some(cell) = &stored.cell {
                *cell
                    .try_borrow_mut()
                    .map_err(|_| NativeError::error("Cannot fill a borrowed PHP reference"))? =
                    array.clone();
            }
            stored.value = array;
        }
        for (object, members) in std::mem::take(&mut self.objects) {
            context.finish_unserialized_object(&object, self.array(members))?;
        }
        Ok(())
    }

    pub(super) fn retain_array_cycles(&mut self, context: &mut dyn Context) {
        let cycles = std::mem::take(&mut self.cyclic_references);
        if !cycles.is_empty() {
            let state = context.state().get_or_default::<RequestCycles>();
            if state.0.len() == state.0.capacity() {
                state.0.retain(|entry| entry.strong_count() != 0);
            }
            state.0.extend(cycles.iter().map(Rc::downgrade));
        }
    }
}

impl Drop for Graph {
    fn drop(&mut self) {
        // If an error stopped construction before a PHP hook could see these
        // arrays, no other code owns the back edges.
        for cell in &self.cyclic_references {
            if let Ok(mut value) = cell.try_borrow_mut() {
                *value = Value::Null;
            }
        }
    }
}

#[derive(Default)]
struct RequestCycles(Vec<Weak<RefCell<Value>>>);

impl Drop for RequestCycles {
    fn drop(&mut self) {
        for cell in self.0.iter().filter_map(Weak::upgrade) {
            if let Ok(mut value) = cell.try_borrow_mut() {
                *value = Value::Null;
            }
        }
    }
}
