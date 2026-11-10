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
