//! Objects and resources: handle semantics (clones share the instance).

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::array::{ArrayKey, PhpArray};
use crate::value::Value;

/// The VM's per-class data. Implemented by other crates; tests use a dummy.
pub trait ClassHandle: Any {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
}

/// A PHP object. Cloning the surrounding `Rc<Object>` shares the instance;
/// every variable holding the object sees the same properties.
pub struct Object {
    pub id: u32,
    pub class: Rc<dyn ClassHandle>,
    /// Reached only through the methods below, so the representation can
    /// change (to slots indexed by declaration order, for example) without
    /// touching the code that uses objects.
    ///
    /// Keys are the ones PHP itself uses: the plain name for a public
    /// property, `\0*\0name` for a protected one and `\0Class\0name` for
    /// a private one. So a parent's and a child's private property of the
    /// same name do not collide, and `(array)`, `var_dump` and `serialize`
    /// can tell the visibility from the key alone.
    pub(crate) properties: RefCell<PhpArray>,
    /// Native state for built-in classes (closures, PDO handles, ...).
    pub native: RefCell<Option<Box<dyn Any>>>,
}

impl Object {
    pub fn new(id: u32, class: Rc<dyn ClassHandle>) -> Self {
        Object {
            id,
            class,
            properties: RefCell::new(PhpArray::new()),
            native: RefCell::new(None),
        }
    }

    /// The value of a property, if the object has it.
    pub fn get(&self, name: &ArrayKey) -> Option<Value> {
        self.properties.try_borrow().ok()?.get(name).cloned()
    }

    pub fn has(&self, name: &ArrayKey) -> bool {
        self.properties
            .try_borrow()
            .is_ok_and(|properties| properties.contains_key(name))
    }

    /// Sets a property, creating it if needed, and returns what it held.
    pub fn set(&self, name: ArrayKey, value: Value) -> Option<Value> {
        self.update(|properties| properties.insert(name, value))
            .flatten()
    }

    /// Removes a property and returns what it held.
    pub fn remove(&self, name: &ArrayKey) -> Option<Value> {
        self.update(|properties| properties.remove(name)).flatten()
    }

    pub fn property_count(&self) -> usize {
        self.properties
            .try_borrow()
            .map_or(0, |properties| properties.len())
    }

    /// A copy of the properties in declaration order, as `(array) $object`,
    /// `foreach` and `var_dump` see them.
    pub fn to_array(&self) -> PhpArray {
        self.properties
            .try_borrow()
            .map_or_else(|_| PhpArray::new(), |properties| properties.clone())
    }

    /// Changes the properties in place, for writes into a property such as
    /// `$this->items[] = $x`, which must not copy the array inside.
    ///
    /// Returns `None` without running `change` if the properties are already
    /// being changed further up the call stack. That cannot happen in
    /// ordinary execution; refusing is safer than the panic a `RefCell`
    /// would raise.
    pub fn update<R>(&self, change: impl FnOnce(&mut PhpArray) -> R) -> Option<R> {
        let mut properties = self.properties.try_borrow_mut().ok()?;
        Some(change(&mut properties))
    }

    /// The key under which PHP stores the private property `name` declared
    /// by `class`.
    pub fn private_key(class: &str, name: &str) -> ArrayKey {
        ArrayKey::str(format!("\0{class}\0{name}").into_bytes())
    }

    /// The key under which PHP stores the protected property `name`.
    pub fn protected_key(name: &str) -> ArrayKey {
        ArrayKey::str(format!("\0*\0{name}").into_bytes())
    }
}

/// Who may see a property, as recorded in its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyScope<'a> {
    Public,
    Protected,
    /// Private to the class with this name.
    Private(&'a [u8]),
}

/// Splits a property key into the property's name and its visibility. The
/// reverse of [`Object::private_key`] and [`Object::protected_key`].
pub fn split_property_key(key: &[u8]) -> (&[u8], PropertyScope<'_>) {
    let [0, rest @ ..] = key else {
        return (key, PropertyScope::Public);
    };
    let Some(end) = rest.iter().position(|&byte| byte == 0) else {
        return (key, PropertyScope::Public);
    };
    let (scope, name) = (&rest[..end], &rest[end + 1..]);
    if scope == b"*" {
        (name, PropertyScope::Protected)
    } else {
        (name, PropertyScope::Private(scope))
    }
}

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Object({}, {})", self.id, self.class.name())
    }
}

/// A PHP resource. `data == None` means the resource is closed
/// (`gettype` then reports `"resource (closed)"`).
pub struct Resource {
    pub id: i64,
    pub kind: &'static str,
    pub data: RefCell<Option<Box<dyn Any>>>,
}

impl Resource {
    pub fn new(id: i64, kind: &'static str, data: Option<Box<dyn Any>>) -> Self {
        Resource {
            id,
            kind,
            data: RefCell::new(data),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.data.borrow().is_none()
    }
}

impl std::fmt::Debug for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Resource({}, {})", self.id, self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArrayKey, Value};

    struct Node;

    impl ClassHandle for Node {
        fn name(&self) -> &str {
            "Node"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// A linked list of a million objects, each holding the next in a
    /// property, alternately directly and through a PHP reference. Freeing
    /// it one stack frame per node would overflow the stack.
    #[test]
    fn dropping_a_long_chain_of_objects_does_not_recurse() {
        let class: Rc<dyn ClassHandle> = Rc::new(Node);
        let mut head = Value::Null;
        for id in 0..1_000_000 {
            let node = Object::new(id, class.clone());
            let next = if id % 2 == 0 {
                head
            } else {
                Value::Reference(Rc::new(RefCell::new(head)))
            };
            node.properties
                .borrow_mut()
                .insert(ArrayKey::str("next"), next);
            head = Value::Object(Rc::new(node));
        }
        drop(head);
    }
}
