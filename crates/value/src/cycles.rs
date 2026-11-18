//! Finding values that only keep each other alive.
//!
//! ```php
//! $a = new stdClass; $b = new stdClass;
//! $a->other = $b; $b->other = $a;
//! unset($a, $b);              // each still holds the other
//! var_dump(gc_collect_cycles()); // int(2)
//! ```
//!
//! Reference counting frees a value when its count reaches zero, which
//! never happens inside a cycle. PHP's collector finds such cycles by "trial
//! deletion", and so does this module, on the `Rc` counts:
//!
//! 1. Starting from every live object, it walks what each object, array and
//!    PHP reference holds, and counts for each of them how many of its
//!    owners it met on the way (its internal count).
//! 2. A value whose `Rc` count is larger than its internal count is held
//!    from outside the walked graph: by a variable, a register, a static, a
//!    native handle. It is alive, and so is everything it holds.
//! 3. Whatever is not reached from such a value is garbage: it is only held
//!    by other garbage.
//!
//! The walk uses an explicit work list, never Rust recursion, and touches
//! each value once, so it costs time in proportion to the number of live
//! objects and the values they hold. It holds one clone of each value it
//! walked, which it subtracts from that value's count.
//!
//! The native state of a built-in object (a closure's captured variables,
//! for example) is opaque here: what it holds counts as held from outside.
//! So a cycle that passes through native state is not found; it is freed at
//! the end of the request.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::array::PhpArray;
use crate::object::Object;
use crate::value::Value;

/// What the walk found to be garbage.
#[derive(Default)]
pub struct Garbage {
    /// Objects held only by garbage, by id.
    pub objects: Vec<Rc<Object>>,
    /// Arrays held only by garbage.
    pub arrays: Vec<Rc<PhpArray>>,
    /// PHP references held only by garbage.
    pub references: Vec<Rc<RefCell<Value>>>,
}

/// One value that can own others.
#[derive(Clone)]
enum Node {
    Object(Rc<Object>),
    Array(Rc<PhpArray>),
    Reference(Rc<RefCell<Value>>),
}

impl Node {
    fn of(value: &Value) -> Option<Node> {
        match value {
            Value::Object(object) => Some(Node::Object(object.clone())),
            Value::Array(array) => Some(Node::Array(array.clone())),
            Value::Reference(cell) => Some(Node::Reference(cell.clone())),
            _ => None,
        }
    }

    /// Identity: the address of the shared allocation.
    fn key(&self) -> usize {
        match self {
            Node::Object(object) => Rc::as_ptr(object) as *const () as usize,
            Node::Array(array) => Rc::as_ptr(array) as *const () as usize,
            Node::Reference(cell) => Rc::as_ptr(cell) as *const () as usize,
        }
    }

    fn strong_count(&self) -> usize {
        match self {
            Node::Object(object) => Rc::strong_count(object),
            Node::Array(array) => Rc::strong_count(array),
            Node::Reference(cell) => Rc::strong_count(cell),
        }
    }

    /// The values this one holds directly, or `None` when they cannot be
    /// seen (borrowed elsewhere, or kept in native state).
    fn children(&self) -> Option<Vec<Node>> {
        match self {
            Node::Object(object) => {
                let native = object.native.try_borrow().ok()?;
                if native.is_some() {
                    return None;
                }
                let properties = object.properties.try_borrow().ok()?;
                Some(properties.values().filter_map(Node::of).collect())
            }
            Node::Array(array) => Some(array.values().filter_map(Node::of).collect()),
            Node::Reference(cell) => {
                let value = cell.try_borrow().ok()?;
                Some(Node::of(&value).into_iter().collect())
            }
        }
    }
}

/// What the walk knows about one value.
struct Entry {
    node: Node,
    /// The values it holds; `None` when they cannot be seen.
    children: Option<Vec<usize>>,
    /// How many of its owners the walk met.
    internal: usize,
    alive: bool,
}

/// Finds the garbage among `objects` and what they hold. The caller passes
/// each object without keeping another handle on it (upgrading the weak
/// handles of a list of live objects one by one, for example), since every
/// handle outside the walk counts as an owner.
pub fn find_garbage(objects: impl IntoIterator<Item = Rc<Object>>) -> Garbage {
    let mut entries: Vec<Entry> = Vec::new();
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut work: Vec<usize> = Vec::new();
    for object in objects {
        let node = Node::Object(object);
        if let std::collections::hash_map::Entry::Vacant(slot) = index_of.entry(node.key()) {
            slot.insert(entries.len());
            work.push(entries.len());
            entries.push(Entry {
                node,
                children: None,
                internal: 0,
                alive: false,
            });
        }
    }
    // 1. Walk everything the objects hold, counting internal owners.
    while let Some(index) = work.pop() {
        let Some(children) = entries[index].node.children() else {
            continue;
        };
        let mut indexes = Vec::with_capacity(children.len());
        for child in children {
            let next = entries.len();
            let child_index = *index_of.entry(child.key()).or_insert(next);
            if child_index == next {
                entries.push(Entry {
                    node: child,
                    children: None,
                    internal: 0,
                    alive: false,
                });
                work.push(child_index);
            }
            entries[child_index].internal += 1;
            indexes.push(child_index);
        }
        entries[index].children = Some(indexes);
    }
    // 2. Values held from outside are alive, with all they hold. The walk
    //    itself holds one clone of each value. A value whose contents could
    //    not be seen keeps what it holds unseen: it stays alive.
    for entry in &mut entries {
        let outside = entry.node.strong_count().saturating_sub(1 + entry.internal);
        entry.alive = outside > 0 || entry.children.is_none();
    }
    work.extend((0..entries.len()).filter(|&index| entries[index].alive));
    while let Some(index) = work.pop() {
        let children = entries[index].children.clone().unwrap_or_default();
        for child in children {
            if !entries[child].alive {
                entries[child].alive = true;
                work.push(child);
            }
        }
    }
    // 3. The rest is garbage.
    let mut garbage = Garbage::default();
    for entry in entries.into_iter().filter(|entry| !entry.alive) {
        match entry.node {
            Node::Object(object) => garbage.objects.push(object),
            Node::Array(array) => garbage.arrays.push(array),
            Node::Reference(cell) => garbage.references.push(cell),
        }
    }
    garbage.objects.sort_by_key(|object| object.id);
    garbage
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::rc::Rc;

    use super::find_garbage;
    use crate::object::{ClassHandle, Object};
    use crate::{ArrayKey, PhpArray, Value};

    struct Node;

    impl ClassHandle for Node {
        fn name(&self) -> &str {
            "Node"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn object(id: u32) -> Rc<Object> {
        Rc::new(Object::new(id, Rc::new(Node)))
    }

    #[test]
    fn two_objects_holding_each_other_are_garbage() {
        let (a, b) = (object(1), object(2));
        a.set(ArrayKey::str("other"), Value::Object(b.clone()));
        b.set(ArrayKey::str("other"), Value::Object(a.clone()));
        let all = vec![a.clone(), b.clone()];
        drop((a, b));
        let garbage = find_garbage(all.iter().cloned());
        assert_eq!(garbage.objects.len(), 0, "the list itself holds them");
        let weak: Vec<_> = all.iter().map(Rc::downgrade).collect();
        drop(all);
        let garbage = find_garbage(weak.iter().filter_map(|object| object.upgrade()));
        assert_eq!(garbage.objects.len(), 2);
    }

    #[test]
    fn a_cycle_held_by_a_variable_is_alive() {
        let (a, b) = (object(1), object(2));
        a.set(ArrayKey::str("other"), Value::Object(b.clone()));
        b.set(ArrayKey::str("other"), Value::Object(a.clone()));
        let weak = [Rc::downgrade(&a), Rc::downgrade(&b)];
        drop(b);
        let garbage = find_garbage(weak.iter().filter_map(|object| object.upgrade()));
        assert!(garbage.objects.is_empty());
        drop(a);
    }

    #[test]
    fn a_cycle_through_an_array_is_found_with_the_array() {
        let a = object(1);
        let mut list = PhpArray::new();
        let _ = list.push(Value::Object(a.clone()));
        a.set(ArrayKey::str("list"), Value::Array(Rc::new(list)));
        let weak = Rc::downgrade(&a);
        drop(a);
        let garbage = find_garbage(weak.upgrade());
        assert_eq!(garbage.objects.len(), 1);
        assert_eq!(garbage.arrays.len(), 1);
    }
}
