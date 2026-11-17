//! Storage transitions and the reference model.

use super::storage::{SCAN_LIMIT, Storage};
use super::{ArrayKey, PhpArray};
use crate::value::Value;

fn k(i: i64) -> ArrayKey {
    ArrayKey::Int(i)
}
fn ks(s: &str) -> ArrayKey {
    ArrayKey::from_value(&Value::from(s)).unwrap()
}
fn vi(i: i64) -> Value {
    Value::Int(i)
}

/// Which lookup an array uses is invisible from outside, so these tests
/// look inside.
fn lookup_name(array: &PhpArray) -> &'static str {
    match &array.storage {
        Storage::List(_) => "packed",
        Storage::Map(map) if map.entries.len() <= SCAN_LIMIT => "scan",
        Storage::Map(_) => "hashed",
    }
}

#[test]
fn a_list_needs_no_index_however_long() {
    let mut list = PhpArray::new();
    for value in 0..10_000 {
        list.push(vi(value)).unwrap();
    }
    assert_eq!(lookup_name(&list), "packed");
    assert_eq!(list.get(&k(9_999)), Some(&vi(9_999)));
    assert_eq!(list.get(&k(10_000)), None);
    assert_eq!(list.get(&k(-1)), None);
    assert_eq!(list.get(&ks("five")), None);

    // Removing entries leaves holes, not an index.
    assert_eq!(list.pop(), Some(vi(9_999)));
    assert_eq!(list.remove(&k(5)), Some(vi(5)));
    assert_eq!(lookup_name(&list), "packed");
    assert_eq!(list.get(&k(5)), None);
    assert_eq!(list.get(&k(6)), Some(&vi(6)));
    assert!(!list.is_list());

    // Shifting renumbers everything, which makes it a list again.
    assert_eq!(list.shift(), Some(vi(0)));
    assert_eq!(lookup_name(&list), "packed");
    assert!(list.is_list());
    assert_eq!(list.get(&k(0)), Some(&vi(1)));
    assert_eq!(list.get(&k(4)), Some(&vi(6)));

    // A key out of order ends it: PHP appends it, it does not slot in.
    list.remove(&k(2));
    list.insert(k(2), vi(-1));
    assert_eq!(lookup_name(&list), "hashed");
    assert_eq!(list.last(), Some((k(2), &vi(-1))));
}

#[test]
fn small_maps_are_scanned_and_large_ones_hashed() {
    let mut map = PhpArray::new();
    for name in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        map.insert(ks(name), Value::from(name));
    }
    assert_eq!(lookup_name(&map), "scan");
    assert_eq!(map.get(&ks("h")), Some(&Value::from("h")));

    map.insert(ks("i"), Value::from("i"));
    assert_eq!(lookup_name(&map), "hashed");
    for name in ["a", "e", "i"] {
        assert_eq!(map.get(&ks(name)), Some(&Value::from(name)));
    }
    assert_eq!(map.get(&ks("z")), None);
}

/// The packed slot must stay as small as Value; Option uses its unused discriminant.
#[test]
fn storage_sizes() {
    use super::storage::Entry;
    assert_eq!(size_of::<Value>(), 16);
    assert_eq!(size_of::<Option<Value>>(), 16);
    assert_eq!(size_of::<Entry>(), 32);
    assert_eq!(size_of::<PhpArray>(), 48);
    println!(
        "PhpArray={} Value={} Option<Value>={} Entry={}",
        size_of::<PhpArray>(),
        size_of::<Value>(),
        size_of::<Option<Value>>(),
        size_of::<Entry>()
    );
}

/// Mutating through either mutable API must not bypass the iterative-drop check.
#[test]
fn mutable_slots_can_introduce_deep_ownership() {
    use std::rc::Rc;
    let mut nested = Value::Null;
    for _ in 0..200_000 {
        let mut array = PhpArray::new();
        array.push(nested).unwrap();
        nested = Value::Array(Rc::new(array));
    }
    let mut holder = PhpArray::from_iter([(ks("nested"), Value::Null)]);
    *holder.iter_mut().next().unwrap().1 = nested;
    let mut list = PhpArray::from_iter([(k(0), Value::Null)]);
    *list.entry_at_mut(0).unwrap().1 = Value::Array(Rc::new(holder));
    drop(list);
}
