//! Paths with more than one property step, read by the interpreter loop.
//!
//! ```php
//! $this->rsm->fieldMappings[$key]       // two properties, then a key
//! $class->fieldMappings[$name]->enumType  // a property after a key
//! isset($this->em->config->cache)
//! ```
//!
//! A site entry (`crate::property_sites::PropertyEntry`) answers one
//! property of one class from one scope. A path with several property steps
//! gets one entry per property step, recorded together in the instruction's
//! site ([`CachedSite::PropertyChain`]) the first time the VM reads the path.
//! The loop then walks the path itself (`calls::fast_property_chains`):
//! each property through its own entry, each key through the array, so every
//! step costs what a site hit costs.
//!
//! The path is a chain when it starts at `$this` or a local, has only
//! declared-name property steps and keys, at least two property steps, and
//! at most [`MOST_STEPS`] steps. Recording walks the path without side
//! effects, up to its last property step; anything an entry cannot answer (a magic or hooked property, a
//! missing or uninitialized one, a key that is not an integer or a string,
//! a value that is not an object where a property follows) records nothing,
//! and the VM keeps reading the path.

use std::rc::Rc;

use phera_bytecode::{PathBase, Step};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};

use crate::property_sites::PropertyEntry;
use crate::unit_caches::CachedSite;
use crate::{AccessPath, Vm};

/// The most steps of a chain the loop walks.
pub(crate) const MOST_STEPS: usize = 6;

/// Whether `steps` make a chain (see the module).
pub(crate) fn is_chain(steps: impl Iterator<Item = Step>) -> bool {
    let mut properties = 0;
    let mut count = 0;
    for step in steps {
        count += 1;
        match step {
            Step::Property(_) => properties += 1,
            Step::Index(_) => {}
            _ => return false,
        }
    }
    properties >= 2 && count <= MOST_STEPS
}

impl Vm {
    /// Records the entries of a chain's property steps in the running
    /// instruction's site, if every step can be answered by one.
    #[cold]
    #[inline(never)]
    pub(crate) fn prepare_chain_site(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        let mut current = match place.base()? {
            PathBase::This { .. } => match self.current().and_then(|frame| frame.this().cloned()) {
                Some(this) => Value::Object(this),
                None => return Ok(()),
            },
            PathBase::Register(register) => match self.read_defined_local(register) {
                Some(value @ (Value::Object(_) | Value::Array(_))) => value,
                _ => return Ok(()),
            },
            _ => return Ok(()),
        };
        let scope = self.current().and_then(|frame| frame.scope().cloned());
        let steps: Vec<Step> = place.steps()?.collect();
        let properties = steps
            .iter()
            .filter(|step| matches!(step, Step::Property(_)))
            .count();
        let mut entries: Vec<Rc<PropertyEntry>> = Vec::with_capacity(properties);
        for step in steps {
            // Once every property has its entry, the keys after the last
            // one do not matter (an `isset` that is false misses one).
            if entries.len() == properties {
                break;
            }
            current = match step {
                Step::Property(name) => {
                    let Value::Object(object) = &current else {
                        return Ok(());
                    };
                    if object.lazy().is_some() {
                        return Ok(());
                    }
                    let Ok(Some(entry)) = self.make_property_entry(object, name) else {
                        return Ok(());
                    };
                    if entry.hooks.is_some() {
                        return Ok(());
                    }
                    let Some(value) = entry.read_matching(object, scope.as_ref()) else {
                        return Ok(());
                    };
                    entries.push(entry);
                    value
                }
                Step::Index(register) => {
                    let key = match &*self.operand(register) {
                        Value::Int(integer) => ArrayKey::Int(*integer),
                        Value::String(string) => ArrayKey::from_string(string.clone()),
                        _ => return Ok(()),
                    };
                    let Value::Array(array) = &current else {
                        return Ok(());
                    };
                    match array.get(&key) {
                        Some(Value::Reference(_)) | None => return Ok(()),
                        Some(value) => value.clone(),
                    }
                }
                _ => return Ok(()),
            };
        }
        if let Some(cell) = self.current().and_then(|frame| frame.site_cache()) {
            *cell.borrow_mut() = CachedSite::PropertyChain(entries.into());
        }
        Ok(())
    }
}
