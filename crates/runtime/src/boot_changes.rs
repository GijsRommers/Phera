//! What the boot wrote into the superglobals, applied to every request.
//!
//! ```php
//! // vendor/symfony/dotenv, during the boot run:
//! $_SERVER['APP_ENV'] = 'prod';
//! Phera\snapshot();
//! // in every request: $_SERVER holds the request's own values, and APP_ENV
//! ```
//!
//! A fork gets the superglobals of its request. Under PHP-FPM the boot code
//! runs after those are set, so what it writes into them stays for the rest
//! of the request: environment loaders put their variables into `$_SERVER`
//! this way. So the changes the boot made (keys added, changed or removed,
//! compared with what the boot run started with) are made again on top of
//! each request's values. The values come from the fork's own copy of the
//! boot's superglobals, so that an object stored there is the fork's.

use phera_value::{ArrayKey, PhpArray, Value};
use phera_vm::{Snapshot, Vm};

/// The superglobals a boot can change and a request replaces.
const REQUEST_SUPERGLOBALS: [&str; 6] =
    ["_SERVER", "_GET", "_POST", "_COOKIE", "_REQUEST", "_FILES"];

/// The keys the boot set or removed, per superglobal.
#[derive(Default)]
pub(crate) struct BootChanges(Vec<(&'static str, Vec<ArrayKey>)>);

/// The values of those keys in one fork, read before its request's
/// superglobals replace them: `None` for a key the boot removed.
pub(crate) struct Carried(Vec<(&'static str, Vec<(ArrayKey, Option<Value>)>)>);

impl BootChanges {
    /// What the boot changed: `before` holds the superglobals it started with.
    pub(crate) fn between(before: &[(&'static str, PhpArray)], snapshot: &Snapshot) -> Self {
        let mut changes = Vec::new();
        for (name, initial) in before {
            let current = match snapshot.global(name) {
                Some(Value::Array(array)) => array,
                _ => continue,
            };
            let mut changed: Vec<ArrayKey> = current
                .iter()
                .filter(|(key, value)| initial.get(key).is_none_or(|old| !same(old, value)))
                .map(|(key, _)| key)
                .collect();
            changed.extend(initial.keys().filter(|key| !current.contains_key(key)));
            if !changed.is_empty() {
                changes.push((*name, changed));
            }
        }
        BootChanges(changes)
    }

    /// The superglobals of a boot run, as the boot started with them.
    pub(crate) fn initial(vm: &Vm) -> Vec<(&'static str, PhpArray)> {
        REQUEST_SUPERGLOBALS
            .iter()
            .filter_map(|name| match vm.global(name) {
                Some(Value::Array(array)) => Some((*name, (*array).clone())),
                _ => None,
            })
            .collect()
    }

    /// The values the changed keys have in a fork, before its request's
    /// superglobals are set.
    pub(crate) fn carry(&self, vm: &Vm) -> Carried {
        let values = self
            .0
            .iter()
            .map(|(name, keys)| {
                let array = match vm.global(name) {
                    Some(Value::Array(array)) => Some(array),
                    _ => None,
                };
                let values = keys
                    .iter()
                    .map(|key| {
                        let value = array.as_ref().and_then(|array| array.get(key)).cloned();
                        (key.clone(), value)
                    })
                    .collect();
                (*name, values)
            })
            .collect();
        Carried(values)
    }
}

impl Carried {
    /// Makes the boot's changes on a fork whose superglobals are set.
    pub(crate) fn apply(self, vm: &mut Vm) {
        for (name, changes) in self.0 {
            let mut array = match vm.global(name) {
                Some(Value::Array(array)) => (*array).clone(),
                _ => PhpArray::new(),
            };
            for (key, value) in changes {
                match value {
                    Some(value) => {
                        array.insert(key, value);
                    }
                    None => {
                        array.remove(&key);
                    }
                }
            }
            vm.set_global(name, Value::from(array));
        }
    }
}

/// Whether the boot left a value as it was: the same string or number.
fn same(old: &Value, new: &Value) -> bool {
    match (old, new) {
        (Value::String(old), Value::String(new)) => old.as_bytes() == new.as_bytes(),
        (Value::Int(old), Value::Int(new)) => old == new,
        (Value::Float(old), Value::Float(new)) => old.to_bits() == new.to_bits(),
        (Value::Bool(old), Value::Bool(new)) => old == new,
        (Value::Null, Value::Null) => true,
        (Value::Array(old), Value::Array(new)) => std::rc::Rc::ptr_eq(old, new),
        _ => false,
    }
}
