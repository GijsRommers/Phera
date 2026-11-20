//! The copier keeps sharing, cycles and ids, and copies only what it must.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::{Fork, NativeFork};
use crate::{ArrayKey, ClassHandle, Object, ObjectIds, PhpArray, Value, WeakObject};

struct Node;

impl ClassHandle for Node {
    fn name(&self) -> &str {
        "Node"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Native state of the tests: a number is copied, a string refused.
struct Natives;

impl NativeFork for Natives {
    fn fork_native(
        &self,
        _object: &Object,
        state: &dyn Any,
        _fork: &mut Fork<'_>,
    ) -> Result<Box<dyn Any>, String> {
        match state.downcast_ref::<i64>() {
            Some(number) => Ok(Box::new(*number)),
            None => Err("an open file".into()),
        }
    }

    fn fork_resource(
        &self,
        _resource: &crate::object::Resource,
        _fork: &mut Fork<'_>,
    ) -> Result<crate::object::Resource, String> {
        Err("a file".into())
    }
}

fn object(ids: &Rc<ObjectIds>) -> Rc<Object> {
    Rc::new(Object::allocate(ids, Rc::new(Node)))
}

fn property(object: &Object, name: &str) -> Value {
    object.get(&ArrayKey::str(name)).unwrap_or_default()
}

fn as_object(value: &Value) -> Rc<Object> {
    match value {
        Value::Object(object) => object.clone(),
        other => panic!("not an object: {other:?}"),
    }
}

/// `$a->self = $a; $root = [$a, $a]`: one copy, still a cycle, same id.
#[test]
fn sharing_cycles_and_ids_survive() {
    let ids = ObjectIds::new();
    let a = object(&ids);
    a.set(ArrayKey::str("self"), Value::Object(a.clone()));
    let mut root = PhpArray::new();
    root.push(Value::Object(a.clone())).ok();
    root.push(Value::Object(a.clone())).ok();
    let root = Value::Array(Rc::new(root));

    let mut fork = Fork::new(&ids, &Natives);
    let copy = fork.value(&root);
    let copies = fork.finish().expect("nothing refused");
    assert_eq!(copies.len(), 1);
    let Value::Array(copy) = copy else { panic!() };
    let first = as_object(copy.get(&ArrayKey::Int(0)).unwrap());
    let second = as_object(copy.get(&ArrayKey::Int(1)).unwrap());
    assert!(Rc::ptr_eq(&first, &second));
    assert!(!Rc::ptr_eq(&first, &a));
    assert_eq!(first.id, a.id);
    assert!(Rc::ptr_eq(&as_object(&property(&first, "self")), &first));
    // Writes to the copy do not reach the original.
    first.set(ArrayKey::str("x"), Value::Int(1));
    assert!(a.get(&ArrayKey::str("x")).is_none());
}

/// An array of scalars is shared; one that holds an object is rebuilt.
#[test]
fn arrays_are_shared_unless_they_hold_identity() {
    let ids = ObjectIds::new();
    let mut scalars = PhpArray::new();
    scalars.push(Value::Int(1)).ok();
    let scalars = Rc::new(scalars);
    let mut nested = PhpArray::new();
    nested.push(Value::Array(scalars.clone())).ok();
    let mut inner = PhpArray::new();
    inner.push(Value::Object(object(&ids))).ok();
    nested.push(Value::Array(Rc::new(inner))).ok();
    let nested = Rc::new(nested);

    let mut fork = Fork::new(&ids, &Natives);
    let Value::Array(copy) = fork.value(&Value::Array(nested.clone())) else {
        panic!()
    };
    fork.finish().expect("nothing refused");
    assert!(!Rc::ptr_eq(&copy, &nested));
    let Some(Value::Array(shared)) = copy.get(&ArrayKey::Int(0)) else {
        panic!()
    };
    assert!(Rc::ptr_eq(shared, &scalars));
}

/// A reference cell is copied once and stays one cell for both holders.
#[test]
fn reference_cells_are_copied_once() {
    let ids = ObjectIds::new();
    let cell = Rc::new(RefCell::new(Value::Int(5)));
    let mut array = PhpArray::new();
    array.push(Value::Reference(cell.clone())).ok();
    array.push(Value::Reference(cell.clone())).ok();
    let mut fork = Fork::new(&ids, &Natives);
    let Value::Array(copy) = fork.value(&Value::Array(Rc::new(array))) else {
        panic!()
    };
    fork.finish().expect("nothing refused");
    let (Some(Value::Reference(a)), Some(Value::Reference(b))) =
        (copy.get(&ArrayKey::Int(0)), copy.get(&ArrayKey::Int(1)))
    else {
        panic!()
    };
    assert!(Rc::ptr_eq(a, b) && !Rc::ptr_eq(a, &cell));
    *a.borrow_mut() = Value::Int(6);
    assert!(matches!(*cell.borrow(), Value::Int(5)));
}

/// A million objects in a chain are copied without deep recursion.
#[test]
fn a_long_chain_is_copied_without_recursion() {
    let ids = ObjectIds::new();
    let mut head = Value::Null;
    for step in 0..1_000_000 {
        let node = object(&ids);
        let next = if step % 2 == 0 {
            head
        } else {
            Value::Reference(Rc::new(RefCell::new(head)))
        };
        node.set(ArrayKey::str("next"), next);
        head = Value::Object(node);
    }
    let mut fork = Fork::new(&ids, &Natives);
    let copy = fork.value(&head);
    assert_eq!(
        fork.finish().map(|copies| copies.len()).ok(),
        Some(1_000_000)
    );
    drop(copy);
}

/// Native state goes through the hook; a refusal names the class.
#[test]
fn native_state_is_forked_or_refused() {
    let ids = ObjectIds::new();
    let counter = object(&ids);
    *counter.native.borrow_mut() = Some(Box::new(41_i64));
    let mut fork = Fork::new(&ids, &Natives);
    let copy = as_object(&fork.value(&Value::Object(counter.clone())));
    fork.finish().expect("numbers are copied");
    assert_eq!(
        copy.native
            .borrow()
            .as_ref()
            .and_then(|n| n.downcast_ref::<i64>()),
        Some(&41)
    );

    let file = object(&ids);
    *file.native.borrow_mut() = Some(Box::new("handle"));
    let mut fork = Fork::new(&ids, &Natives);
    fork.value(&Value::Object(file.clone()));
    let refusal = fork.finish().expect_err("strings are refused");
    assert_eq!(refusal.class, "Node");
    assert_eq!(refusal.address, Rc::as_ptr(&file) as usize);
}

/// A weak handle follows its object into the fork, or dies with it.
#[test]
fn weak_handles_follow_copies() {
    let ids = ObjectIds::new();
    let target = object(&ids);
    let handle = WeakObject::new(&target);
    let mut fork = Fork::new(&ids, &Natives);
    let copy = as_object(&fork.value(&Value::Object(target.clone())));
    let copied_handle = fork.weak(&handle);
    fork.finish().expect("nothing refused");
    assert!(Rc::ptr_eq(&copied_handle.upgrade().unwrap(), &copy));

    let gone = WeakObject::new(&object(&ids));
    let mut fork = Fork::new(&ids, &Natives);
    assert!(fork.weak(&gone).upgrade().is_none());
}
