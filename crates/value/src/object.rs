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
    fn is_enum(&self) -> bool {
        false
    }
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
    /// Gives the id back to its pool when the object is freed. A separate
    /// field rather than a `Drop` on `Object` itself, because a type with
    /// its own `Drop` cannot have its fields moved out, and freeing long
    /// chains of objects without recursion relies on moving the properties
    /// out. The underscore says the field is never read: it exists only
    /// to be dropped.
    _lease: Option<IdLease>,
}

impl Object {
    /// An object with an id chosen by the caller, which is never reused.
    pub fn new(id: u32, class: Rc<dyn ClassHandle>) -> Self {
        Object {
            id,
            class,
            properties: RefCell::new(PhpArray::new()),
            native: RefCell::new(None),
            _lease: None,
        }
    }

    /// An object with the next id from `ids`. The id returns to the pool
    /// when the object is freed, so the pool never has to look for dead
    /// objects.
    pub fn allocate(ids: &Rc<ObjectIds>, class: Rc<dyn ClassHandle>) -> Self {
        let lease = IdLease {
            id: ids.take(),
            pool: ids.clone(),
        };
        Object {
            id: lease.id,
            class,
            properties: RefCell::new(PhpArray::new()),
            native: RefCell::new(None),
            _lease: Some(lease),
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

/// The ids of a request's objects (the `#3` in `var_dump`).
///
/// PHP numbers objects from 1 and reuses the id of a freed object, most
/// recently freed first. Scripts see this through `spl_object_id` and
/// `var_dump`, so the order matters.
#[derive(Debug, Default)]
pub struct ObjectIds {
    state: RefCell<IdState>,
}

#[derive(Debug, Default)]
struct IdState {
    /// The highest id handed out so far.
    highest: u32,
    /// Ids of freed objects; the last one pushed is reused first.
    free: Vec<u32>,
}

impl ObjectIds {
    pub fn new() -> Rc<Self> {
        Rc::new(ObjectIds::default())
    }

    fn take(&self) -> u32 {
        let mut state = self.state.borrow_mut();
        if let Some(id) = state.free.pop() {
            return id;
        }
        state.highest = state.highest.saturating_add(1);
        state.highest
    }

    fn give_back(&self, id: u32) {
        // Nothing borrows the state across a call that could free an
        // object, so this cannot fail; if it ever did, the id would simply
        // not be reused.
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.free.push(id);
        }
    }

    /// How many objects are alive.
    pub fn live(&self) -> usize {
        let state = self.state.borrow();
        state.highest as usize - state.free.len()
    }
}

/// Returns an object's id to the pool when dropped.
struct IdLease {
    id: u32,
    pool: Rc<ObjectIds>,
}

impl Drop for IdLease {
    fn drop(&mut self) {
        self.pool.give_back(self.id);
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

    /// PHP gives a new object the id of the most recently freed one.
    #[test]
    fn freed_ids_are_reused_last_freed_first() {
        let class: Rc<dyn ClassHandle> = Rc::new(Node);
        let ids = ObjectIds::new();
        let first = Object::allocate(&ids, class.clone());
        let second = Object::allocate(&ids, class.clone());
        let third = Object::allocate(&ids, class.clone());
        assert_eq!((first.id, second.id, third.id), (1, 2, 3));

        drop(first);
        drop(second);
        let reused: Vec<u32> = (0..3)
            .map(|_| {
                let object = Object::allocate(&ids, class.clone());
                let id = object.id;
                // Keep it alive so the next one cannot take the same id.
                std::mem::forget(object);
                id
            })
            .collect();
        assert_eq!(reused, vec![2, 1, 4]);
        assert_eq!(ids.live(), 4);
    }

    /// Ids come back through the non-recursive drop of a long chain too.
    #[test]
    fn a_dropped_chain_returns_every_id() {
        let class: Rc<dyn ClassHandle> = Rc::new(Node);
        let ids = ObjectIds::new();
        let mut head = Value::Null;
        for _ in 0..100_000 {
            let node = Object::allocate(&ids, class.clone());
            node.set(ArrayKey::str("next"), head);
            head = Value::Object(Rc::new(node));
        }
        assert_eq!(ids.live(), 100_000);
        drop(head);
        assert_eq!(ids.live(), 0);
    }
}
