//! Objects and resources: handle semantics (clones share the instance).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::array::{ArrayKey, PhpArray};
use crate::destruction::{Destruction, DoomedObjects};
use crate::value::Value;

/// Scalar casts supported by PHP's native classes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObjectCast {
    Bool,
    Int,
    Float,
    /// An int when the value fits one, a float otherwise: what `array_sum()` asks for.
    Number,
    String,
}

/// The VM's per-class data. Implemented by other crates; tests use a dummy.
pub trait ClassHandle: Any {
    fn name(&self) -> &str;
    fn is_instance_of(&self, name: &str) -> bool {
        self.name().eq_ignore_ascii_case(name)
    }

    fn is_traversable(&self) -> bool {
        self.is_instance_of("Traversable")
    }

    fn is_countable(&self) -> bool {
        self.is_instance_of("Countable")
    }

    /// Whether the class declares a property, including uninitialized properties.
    fn has_property(&self, _name: &str) -> bool {
        false
    }

    /// Whether the object's property table is supplied by native code.
    fn has_native_properties(&self) -> bool {
        false
    }

    /// A native object may supply its own scalar conversion.
    fn cast(&self, _object: &Object, _target: ObjectCast) -> Option<Value> {
        None
    }

    fn as_any(&self) -> &dyn Any;

    /// A lazy backing table still exposes declared properties in class order.
    fn order_lazy_properties(&self, properties: PhpArray) -> PhpArray {
        properties
    }
    fn is_enum(&self) -> bool {
        false
    }
}

/// Anonymous classes expose their full identity through get_class, while diagnostics use the prefix.
pub fn class_display_name(name: &str) -> &str {
    name.split_once('\0').map_or(name, |(prefix, _)| prefix)
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
    /// Whether `__destruct` still has to run; see `crate::destruction`.
    pub(crate) destruction: Cell<Destruction>,
    /// Values `WeakMap`s keep for this object; see `crate::attachments`.
    pub(crate) attachments: crate::attachments::Attachments,
    /// The pool the id goes back to when the object is freed. Moved, not
    /// copied, into the object that waits for its destructor, so the id
    /// stays taken until that object is really gone. `None` for an object
    /// made without a pool. Declared after the fields that hold values:
    /// fields are dropped in order, and the id goes back last (see
    /// [`PoolHold`]).
    pub(crate) pool: Option<PoolHold>,
}

impl Object {
    /// An object with an id chosen by the caller, which is never reused.
    pub fn new(id: u32, class: Rc<dyn ClassHandle>) -> Self {
        Object {
            id,
            class,
            properties: RefCell::new(PhpArray::new()),
            native: RefCell::new(None),
            destruction: Cell::new(Destruction::Never),
            attachments: Cell::new(None),
            pool: None,
        }
    }

    /// An object with the next id from `ids`. The id returns to the pool
    /// when the object is freed, so the pool never has to look for dead
    /// objects.
    pub fn allocate(ids: &Rc<ObjectIds>, class: Rc<dyn ClassHandle>) -> Self {
        Self::allocate_with_properties(ids, class, PhpArray::new())
    }

    /// Install the property table before publishing the object.
    #[inline]
    pub fn allocate_with_properties(
        ids: &Rc<ObjectIds>,
        class: Rc<dyn ClassHandle>,
        properties: PhpArray,
    ) -> Self {
        let id = ids.take();
        Object {
            id,
            class,
            properties: RefCell::new(properties),
            native: RefCell::new(None),
            destruction: Cell::new(Destruction::Never),
            attachments: Cell::new(None),
            pool: Some(PoolHold {
                pool: ids.clone(),
                id,
                incarnated: false,
            }),
        }
    }

    /// PHP's stable string representation of an object handle.
    pub fn handle_hash(&self) -> String {
        format!("{:016x}{:016x}", self.id, 0)
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

    /// Reads the stored table without cloning it or entering a PHP callback.
    pub fn with_properties<R>(&self, read: impl FnOnce(&PhpArray) -> R) -> Option<R> {
        let properties = self.properties.try_borrow().ok()?;
        Some(read(&properties))
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
    /// Objects whose destructor is due; see `crate::destruction`.
    pub(crate) doomed: DoomedObjects,
    /// The object that now carries each id whose object was handed over
    /// for its destructor, with the identity (address) of the object it
    /// took over from; see `crate::weak_object`.
    pub(crate) incarnations: RefCell<HashMap<u32, (usize, Weak<Object>)>>,
}

#[derive(Debug, Default)]
struct IdState {
    /// The highest id handed out so far.
    highest: u32,
    /// Ids of freed objects; the last one pushed is reused first.
    free: Vec<u32>,
    /// PHP stops reusing ids while it destroys the objects left at the end
    /// of a request.
    no_reuse: bool,
}

impl ObjectIds {
    pub fn new() -> Rc<Self> {
        Rc::new(ObjectIds::default())
    }

    /// A free id.
    pub(crate) fn take(&self) -> u32 {
        let mut state = self.state.borrow_mut();
        if !state.no_reuse
            && let Some(id) = state.free.pop()
        {
            return id;
        }
        state.highest = state.highest.saturating_add(1);
        state.highest
    }

    /// An object is gone, with what it held: its id is free for reuse. An
    /// object that took over another's contents for its destructor is
    /// unregistered too.
    #[inline]
    fn give_back(&self, id: u32, incarnated: bool) {
        // Nothing borrows the state across a call that could free an
        // object, so this cannot fail; if it ever did, the id would simply
        // not be reused.
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.free.push(id);
        }
        if incarnated && let Ok(mut incarnations) = self.incarnations.try_borrow_mut() {
            incarnations.remove(&id);
        }
    }

    /// From now on every object gets a new id.
    pub fn stop_reuse(&self) {
        self.state.borrow_mut().no_reuse = true;
    }

    /// How many objects are alive.
    pub fn live(&self) -> usize {
        let state = self.state.borrow();
        state.highest as usize - state.free.len()
    }
}

/// An object's hold on its request's id pool; the last field of
/// [`Object`].
///
/// PHP frees what an object holds before it gives back the object's id, so
/// the outer object's id is the next to be reused:
///
/// ```php
/// $outer = new Box(new Item);   // #1 holds #2
/// $outer = null;                // frees #2, then #1
/// var_dump(new Item);           // #1
/// ```
///
/// Rust drops an object's fields in order: the values first (whatever they
/// free gives back its own id in between), and this hold last, which gives
/// the object's id back. The hold carries the id itself, so freeing an
/// object costs one return of an id and nothing else.
#[derive(Debug)]
pub(crate) struct PoolHold {
    pub(crate) pool: Rc<ObjectIds>,
    id: u32,
    /// The object took over another's contents for its destructor (see
    /// `crate::weak_object`).
    pub(crate) incarnated: bool,
}

impl std::ops::Deref for PoolHold {
    type Target = Rc<ObjectIds>;

    fn deref(&self) -> &Rc<ObjectIds> {
        &self.pool
    }
}

impl Drop for PoolHold {
    #[inline]
    fn drop(&mut self) {
        self.pool.give_back(self.id, self.incarnated);
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
    let Some(end) = rest.iter().rposition(|&byte| byte == 0) else {
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
