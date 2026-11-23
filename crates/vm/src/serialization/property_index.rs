//! A wide class can restore thousands of declared properties without rescanning them.
//!
//! When `unserialize()` sees `"name"` for a private `$name`, it finds the
//! declaration by its unmangled name and installs the value at its mangled key.

use std::{collections::HashMap, rc::Rc};

use phera_value::ArrayKey;

use crate::Vm;
use crate::classes::{Class, class::PropertyDefault};

#[derive(Default)]
pub(crate) struct PropertyCache(HashMap<String, Rc<PropertyIndex>>);

#[derive(Default)]
struct PropertyIndex {
    by_key: HashMap<ArrayKey, usize>,
    by_name: HashMap<Vec<u8>, usize>,
}

pub(super) struct DeclaredProperties<'a> {
    properties: &'a [PropertyDefault],
    index: Option<Rc<PropertyIndex>>,
}

impl<'a> DeclaredProperties<'a> {
    pub(super) fn new(vm: &mut Vm, class: &'a Class) -> Self {
        let index = if class.defaults.len() > 8 {
            let cache = vm.state_mut().get_or_default::<PropertyCache>();
            let index = match cache.0.get(class.name.as_str()) {
                Some(index) => index.clone(),
                None => {
                    let mut index = PropertyIndex::default();
                    for (position, property) in class.defaults.iter().enumerate() {
                        index.by_key.insert(property.key.clone(), position);
                        index
                            .by_name
                            .insert(property.name.as_bytes().into(), position);
                    }
                    let index = Rc::new(index);
                    cache.0.insert(class.name.clone(), index.clone());
                    index
                }
            };
            Some(index)
        } else {
            None
        };
        Self {
            properties: &class.defaults,
            index,
        }
    }

    pub(super) fn key(&self, key: &ArrayKey) -> Option<&'a PropertyDefault> {
        match &self.index {
            Some(index) => self.properties.get(*index.by_key.get(key)?),
            None => self.properties.iter().find(|property| property.key == *key),
        }
    }

    pub(super) fn name(&self, name: &[u8]) -> Option<&'a PropertyDefault> {
        match &self.index {
            Some(index) => self.properties.get(*index.by_name.get(name)?),
            None => self
                .properties
                .iter()
                .rev()
                .find(|property| property.name.as_bytes() == name),
        }
    }
}
