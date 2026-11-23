//! Where a refused object was found, for the exception message.
//!
//! ```php
//! $app->log = fopen('php://stderr', 'w');
//! Phera\snapshot();
//! // Phera\SnapshotException: Phera\snapshot(): cannot keep stream
//! //   ($app->log): an open resource belongs to one request
//! ```
//!
//! Only a refusal needs the path, so it is searched for afterwards, breadth
//! first from the roots (global variables, static properties, the local
//! variables of every running function), which gives the shortest path.
//! Each object and array is visited once.

use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use phera_value::{ArrayKey, Value, split_property_key};

use crate::Vm;

impl Vm {
    /// The shortest way from a root to the object or resource at `address`.
    pub(crate) fn path_to(&self, address: usize) -> Option<String> {
        if address == 0 {
            return None;
        }
        let mut queue: VecDeque<(Value, String)> = self.snapshot_roots().into();
        let mut seen = HashSet::new();
        while let Some((value, path)) = queue.pop_front() {
            match value {
                Value::Object(object) => {
                    let here = Rc::as_ptr(&object) as usize;
                    if here == address {
                        return Some(path);
                    }
                    if !seen.insert(here) {
                        continue;
                    }
                    for (key, value) in object.to_array().iter() {
                        queue.push_back((value.clone(), format!("{path}->{}", property(&key))));
                    }
                    for (index, value) in crate::closures::captured_values(&object)
                        .into_iter()
                        .enumerate()
                    {
                        queue.push_back((value, format!("{path} (captured variable {index})")));
                    }
                }
                Value::Resource(resource) if Rc::as_ptr(&resource) as usize == address => {
                    return Some(path);
                }
                Value::Array(array) => {
                    if seen.insert(Rc::as_ptr(&array) as usize) {
                        for (key, value) in array.iter() {
                            queue.push_back((value.clone(), format!("{path}[{}]", element(&key))));
                        }
                    }
                }
                Value::Reference(cell) => {
                    let inner = cell.try_borrow().map(|value| value.clone());
                    queue.push_back((inner.unwrap_or_default(), path));
                }
                _ => {}
            }
        }
        None
    }

    /// Global variables, static properties and the locals of the running
    /// functions, each with its name.
    fn snapshot_roots(&self) -> Vec<(Value, String)> {
        let mut roots: Vec<(Value, String)> = self
            .globals
            .iter()
            .map(|(name, value)| (value.clone(), format!("${name}")))
            .collect();
        for class in self.classes.local_classes() {
            for property in &class.static_properties {
                if let Some(storage) = self.class_states.existing_static(property) {
                    let value = Value::Reference(storage.reference.borrow().clone());
                    roots.push((value, format!("{}::${}", class.name, property.name)));
                }
            }
        }
        for frame in self.frames() {
            let function = frame.function();
            let unit = frame.unit();
            for (slot, name) in function.locals.iter().enumerate() {
                if let (Some(Some(value)), Some(name)) = (
                    frame.slots(&self.registers).get(slot),
                    unit.names.get(name.index()),
                ) {
                    let function = if function.name.is_empty() {
                        "{main}"
                    } else {
                        function.name.as_str()
                    };
                    roots.push((value.clone(), format!("${name} in {function}")));
                }
            }
        }
        roots
    }
}

fn property(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(number) => number.to_string(),
        ArrayKey::Interned(name) => {
            String::from_utf8_lossy(split_property_key(name.as_bytes()).0).into_owned()
        }
        ArrayKey::String(name) => {
            String::from_utf8_lossy(split_property_key(name.as_bytes()).0).into_owned()
        }
    }
}

fn element(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(number) => number.to_string(),
        ArrayKey::Interned(name) => format!("'{}'", name.to_string_lossy()),
        ArrayKey::String(name) => format!("'{}'", name.to_string_lossy()),
    }
}
