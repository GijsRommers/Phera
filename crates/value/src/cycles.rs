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
//! A WeakMap attachment contributes to the internal count of its value.
//! Reachability through the attachment requires both the map and its key.
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
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

use crate::array::PhpArray;
use crate::object::Object;
use crate::value::Value;

mod weak_edges;
use weak_edges::{Dependencies, WeakEdge};

/// What the walk found to be garbage.
#[derive(Default)]
pub struct Garbage {
    /// Objects held only by garbage, by id.
    pub objects: Vec<Rc<Object>>,
    /// Arrays held only by garbage.
    pub arrays: Vec<Rc<PhpArray>>,
    /// PHP references held only by garbage.
    pub references: Vec<Rc<RefCell<Value>>>,
    /// Attachments on reachable keys whose maps are garbage.
    pub inactive_attachments: Vec<(Rc<Object>, usize)>,
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

    /// Calls `visit` with each value this one holds directly. `false` when
    /// they cannot be seen (borrowed elsewhere, or kept in native state).
    fn visit_children(&self, mut visit: impl FnMut(&Value)) -> bool {
        match self {
            Node::Object(object) => {
                let Ok(native) = object.native.try_borrow_mut() else {
                    return false;
                };
                let Ok(properties) = object.properties.try_borrow() else {
                    return false;
                };
                if let Some(native) = native.as_deref()
                    && !object.class.trace_native_values(native, &mut visit)
                {
                    return false;
                }
                properties.values().for_each(visit);
                true
            }
            Node::Array(array) => {
                array.values().for_each(visit);
                true
            }
            Node::Reference(cell) => {
                let Ok(value) = cell.try_borrow() else {
                    return false;
                };
                visit(&value);
                true
            }
        }
    }
}

/// The identity of a value that can own others, without a handle on it.
fn key_of(value: &Value) -> Option<usize> {
    match value {
        Value::Object(object) => Some(Rc::as_ptr(object) as *const () as usize),
        Value::Array(array) => Some(Rc::as_ptr(array) as *const () as usize),
        Value::Reference(cell) => Some(Rc::as_ptr(cell) as *const () as usize),
        _ => None,
    }
}

/// Hashes the address of a value. Addresses are already spread out, so
/// one multiplication does what the default hasher's rounds would.
#[derive(Default)]
struct AddressHasher(u64);

impl Hasher for AddressHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 << 8 | u64::from(*byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_usize(&mut self, address: usize) {
        self.0 = (address as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// What the walk knows about one value.
struct Entry {
    node: Node,
    /// Where the values it holds are in the walk's list of edges; `None`
    /// when they cannot be seen.
    edges: Option<std::ops::Range<usize>>,
    /// How many of its owners the walk met.
    internal: usize,
    alive: bool,
}

/// The values walked so far.
#[derive(Default)]
struct Walk {
    entries: Vec<Entry>,
    index_of: HashMap<usize, usize, BuildHasherDefault<AddressHasher>>,
    /// Who holds whom: the indexes of the values each entry holds.
    edges: Vec<usize>,
    weak_edges: Vec<WeakEdge>,
    work: Vec<usize>,
}

impl Walk {
    /// The entry for `node`, made (and queued for walking) if it is new.
    fn enter(&mut self, key: usize, node: impl FnOnce() -> Node) -> usize {
        let next = self.entries.len();
        let index = *self.index_of.entry(key).or_insert(next);
        if index == next {
            self.entries.push(Entry {
                node: node(),
                edges: None,
                internal: 0,
                alive: false,
            });
            self.work.push(index);
        }
        index
    }

    fn count_owner(&mut self, key: usize, value: &Value) -> Option<usize> {
        let child = match self.index_of.get(&key) {
            Some(&known) => known,
            None => {
                if matches!(value, Value::Object(object) if is_leaf(object)) {
                    return None;
                }
                let node = Node::of(value)?;
                self.enter(key, || node)
            }
        };
        self.entries[child].internal += 1;
        Some(child)
    }
}

/// Finds the garbage among `objects` and what they hold. The caller passes
/// each object without keeping another handle on it (upgrading the weak
/// handles of a list of live objects one by one, for example), since every
/// handle outside the walk counts as an owner.
pub fn find_garbage(objects: impl IntoIterator<Item = Rc<Object>>) -> Garbage {
    let mut walk = Walk::default();
    // An object that holds no array, object or reference cannot be part of
    // a cycle: it stays out of the walk, which keeps a large heap of plain
    // objects cheap, and is counted at the end if garbage holds it.
    for object in objects.into_iter().filter(|object| !is_leaf(object)) {
        let node = Node::Object(object);
        walk.enter(node.key(), || node);
    }
    // 1. Walk everything the objects hold, counting internal owners.
    while let Some(index) = walk.work.pop() {
        let node = walk.entries[index].node.clone();
        let start = walk.edges.len();
        let seen = node.visit_children(|value| {
            if let Some(key) = key_of(value)
                && let Some(child) = walk.count_owner(key, value)
            {
                walk.edges.push(child);
            }
        });
        let end = walk.edges.len();
        walk.entries[index].edges = seen.then_some(start..end);
        if let Node::Object(object) = node {
            object.visit_attachments(|owner, value| {
                if let Some(address) = key_of(value) {
                    let child = walk.count_owner(address, value);
                    walk.weak_edges.push(WeakEdge {
                        key: index,
                        owner,
                        child,
                    });
                }
            });
        }
    }
    let Walk {
        mut entries,
        edges,
        weak_edges,
        mut work,
        index_of,
    } = walk;
    let dependencies = Dependencies::new(entries.len(), &weak_edges, &index_of);
    // 2. Values held from outside are alive, with all they hold. The walk
    //    holds one handle on each value. A value whose contents could not
    //    be seen keeps what it holds unseen: it stays alive.
    for entry in &mut entries {
        let outside = entry.node.strong_count().saturating_sub(1 + entry.internal);
        entry.alive = outside > 0 || entry.edges.is_none();
    }
    work.extend((0..entries.len()).filter(|&index| entries[index].alive));
    while let Some(index) = work.pop() {
        let held = entries[index].edges.clone().unwrap_or_default();
        for &child in &edges[held] {
            if !entries[child].alive {
                entries[child].alive = true;
                work.push(child);
            }
        }
        dependencies.visit(index, |other, child| {
            if other.is_none_or(|other| entries[other].alive) && !entries[child].alive {
                entries[child].alive = true;
                work.push(child);
            }
        });
    }
    // 3. The rest is garbage, with the leaf objects only garbage holds.
    let mut garbage = Garbage::default();
    for edge in weak_edges {
        if entries[edge.key].alive
            && index_of
                .get(&edge.owner)
                .is_some_and(|&owner| !entries[owner].alive)
            && let Node::Object(key) = &entries[edge.key].node
        {
            garbage.inactive_attachments.push((key.clone(), edge.owner));
        }
    }
    for entry in entries.into_iter().filter(|entry| !entry.alive) {
        match entry.node {
            Node::Object(object) => garbage.objects.push(object),
            Node::Array(array) => garbage.arrays.push(array),
            Node::Reference(cell) => garbage.references.push(cell),
        }
    }
    let leaves = held_leaves(&garbage, &index_of);
    garbage.objects.extend(leaves);
    garbage.objects.sort_by_key(|object| object.id);
    garbage
}

/// Whether an object holds nothing that could lead back to it.
fn is_leaf(object: &Object) -> bool {
    let Ok(native) = object.native.try_borrow() else {
        return false;
    };
    let Ok(properties) = object.properties.try_borrow() else {
        return false;
    };
    native.is_none()
        && !object.has_attachments()
        && !properties.values().any(|value| {
            matches!(
                value,
                Value::Array(_) | Value::Object(_) | Value::Reference(_)
            )
        })
}

/// The leaf objects that only `garbage` holds: every handle on them comes
/// from a garbage value.
fn held_leaves(
    garbage: &Garbage,
    known: &HashMap<usize, usize, BuildHasherDefault<AddressHasher>>,
) -> Vec<Rc<Object>> {
    let mut held: HashMap<usize, (Rc<Object>, usize), BuildHasherDefault<AddressHasher>> =
        HashMap::default();
    let mut tally = |value: &Value| {
        if let Value::Object(object) = value
            && !known.contains_key(&object.address())
            && is_leaf(object)
        {
            let entry = held
                .entry(Rc::as_ptr(object) as *const () as usize)
                .or_insert_with(|| (object.clone(), 0));
            entry.1 += 1;
        }
    };
    for object in &garbage.objects {
        Node::Object(object.clone()).visit_children(&mut tally);
        object.visit_attachments(|_, value| tally(value));
    }
    for (key, owner) in &garbage.inactive_attachments {
        key.visit_attachments(|candidate, value| {
            if candidate == *owner {
                tally(value);
            }
        });
    }
    for array in &garbage.arrays {
        array.values().for_each(&mut tally);
    }
    for cell in &garbage.references {
        if let Ok(value) = cell.try_borrow() {
            tally(&value);
        }
    }
    held.into_values()
        .filter(|(object, owners)| Rc::strong_count(object) == owners + 1)
        .map(|(object, _)| object)
        .collect()
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
