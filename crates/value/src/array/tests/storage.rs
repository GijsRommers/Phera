//! Which storage shape an array has, and when it changes.

use super::{k, ks, vi};
use crate::array::PhpArray;
use crate::array::storage::{SCAN_LIMIT, Storage};
use crate::value::Value;

/// Which lookup an array uses is invisible from outside, so these tests
/// look inside.
fn lookup_name(array: &PhpArray) -> &'static str {
    match &array.storage {
        Storage::List(_) => "packed",
        Storage::Map(map) if map.entries.len() <= SCAN_LIMIT => "scan",
        Storage::Map(_) => "hashed",
    }
}

/// A list needs no table until a key breaks positional lookup, even after unset.
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

/// The measured scan threshold must switch to a table without losing any key.
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

/// A list slot is as small as a value: `Option<Value>` uses a spare tag of `Value`.
#[test]
fn storage_sizes() {
    use crate::array::storage::Entry;
    assert_eq!(size_of::<Value>(), 16);
    assert_eq!(size_of::<Option<Value>>(), 16);
    assert_eq!(size_of::<Entry>(), 32);
    assert_eq!(size_of::<PhpArray>(), 56);
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

/// Shift renumbers integer maps into lists, including maps with deleted string keys.
#[test]
fn shift_demotes_maps_without_changing_live_order() {
    for entries in [
        vec![(k(9), vi(1)), (k(-5), vi(2)), (k(8), vi(3))],
        vec![(ks("name"), vi(1)), (k(5), vi(2)), (k(1), vi(3))],
    ] {
        let mut array = PhpArray::from_iter(entries);
        assert!(matches!(array.storage, Storage::Map(_)));
        assert_eq!(array.shift(), Some(vi(1)));
        assert!(matches!(array.storage, Storage::List(_)) && array.list_start == 0);
        assert_eq!(
            array.iter().collect::<Vec<_>>(),
            vec![(k(0), &vi(2)), (k(1), &vi(3))]
        );
        assert_eq!(array.push(vi(4)).unwrap(), k(2));
    }
}

/// A trailing unset must not reuse a foreach position (php-src gh13178_5).
#[test]
fn deleting_the_last_slot_keeps_its_position_until_append() {
    let mut array = PhpArray::from_iter([(k(0), vi(0)), (k(1), vi(1)), (k(2), vi(2))]);
    array.remove(&k(2));
    assert_eq!(array.storage.len(), 3);
    assert_eq!(array.push(vi(3)).unwrap(), k(3));
    assert!(array.entry_at(2).is_none());
    assert_eq!(array.entry_at(3), Some((k(3), &vi(3))));
    assert_eq!(array.next_position(1), Some(3));
}

/// Entries pushed in order, with their keys distinct, become the array as
/// they are: a map in their order, a list when the keys are 0, 1, 2, and the
/// next free key after the largest integer key.
#[test]
fn distinct_entries_build_the_array_as_pushed() {
    let map = PhpArray::build_distinct(3, |entries| {
        entries.push(ks("name"), vi(1));
        entries.push(k(7), vi(2));
        entries.push(ks("price"), vi(3));
    });
    assert_eq!(map.len(), 3);
    assert_eq!(
        map.keys().collect::<Vec<_>>(),
        vec![ks("name"), k(7), ks("price")]
    );
    assert_eq!(map.get(&ks("price")), Some(&vi(3)));
    assert_eq!(map.next_free_index(), Some(8));
    assert!(!map.is_list());

    let list = PhpArray::build_distinct(0, |entries| {
        entries.push(k(0), vi(5));
        entries.push(k(1), vi(6));
    });
    assert_eq!(lookup_name(&list), "packed");
    assert!(list.is_list());
    assert_eq!(list.next_free_index(), Some(2));

    assert!(PhpArray::build_distinct(4, |_| {}).is_empty());
}

/// 1-indexed, 2-indexed, and negative-offset sequential arrays stay packed
/// with zero `PhpArray` size increase and accurate `array_is_list()` semantics.
#[test]
fn offset_lists_stay_packed_without_hashing() {
    let mut one_based = PhpArray::new();
    for i in 1..=200 {
        assert_eq!(one_based.insert_int(i, vi(i * 10)), None);
    }
    assert_eq!(lookup_name(&one_based), "packed");
    assert!(!one_based.is_list());
    assert_eq!(one_based.get_int(0), None);
    assert_eq!(one_based.get_int(1), Some(&vi(10)));
    assert_eq!(one_based.get_int(200), Some(&vi(2000)));
    assert_eq!(one_based.push(vi(2010)).unwrap(), k(201));
    assert_eq!(lookup_name(&one_based), "packed");
    assert_eq!(one_based.pop(), Some(vi(2010)));
    assert_eq!(one_based.next_free_index(), Some(201));

    let neg = PhpArray::from_indexed_values(-3, [0, 1, 2, 3, 4].into_iter().map(vi)).unwrap();
    assert_eq!(lookup_name(&neg), "packed");
    assert!(!neg.is_list());
    assert_eq!(neg.get_int(-3), Some(&vi(0)));
    assert_eq!(neg.get_int(1), Some(&vi(4)));
    assert_eq!(neg.next_free_index(), Some(2));
}
