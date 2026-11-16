//! Objects and resources: handle semantics (clones share the instance).

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::array::PhpArray;

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
    pub properties: RefCell<PhpArray>,
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
