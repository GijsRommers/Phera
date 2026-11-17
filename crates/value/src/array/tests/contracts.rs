//! Behaviour PHP code can observe: order, next free key, pop and shift,
//! foreach positions, the internal pointer, copy-on-write.

use super::{k, ks, vi};
use crate::array::{ArrayKey, PhpArray};
use crate::value::Value;
use std::rc::Rc;

fn int_keys(array: &PhpArray) -> Vec<i64> {
    array
        .keys()
        .map(|key| match key {
            ArrayKey::Int(i) => i,
            ArrayKey::String(_) => panic!("expected integer keys only"),
        })
        .collect()
}

fn int_values(array: &PhpArray) -> Vec<i64> {
    array
        .values()
        .map(|value| match value {
            Value::Int(i) => *i,
            other => panic!("expected integer values only, got {other:?}"),
        })
        .collect()
}

/// Compaction closes holes but keeps the order and keys of what survives.
#[test]
fn order_preserved_after_removals_and_compaction() {
    let mut a = PhpArray::new();
    for i in 0..40 {
        a.insert(k(i * 2), vi(i));
    }
    // Unset every other entry: 20 holes, 20 survivors.
    for i in (1..40).step_by(2) {
        assert!(a.remove(&k(i * 2)).is_some());
    }
    assert_eq!(a.len(), 20);
    assert_eq!(
        int_keys(&a),
        (0..40).step_by(2).map(|i| i * 2).collect::<Vec<_>>()
    );

    // More holes than live entries; unset alone never compacts...
    for i in (12..40).step_by(2) {
        a.remove(&k(i * 2));
    }
    assert_eq!(a.len(), 6);
    assert_eq!(a.storage.len(), 40);
    // ...but the next append does.
    a.push(vi(99)).unwrap();
    assert_eq!(a.storage.len(), 7);
    assert_eq!(int_keys(&a), vec![0, 4, 8, 12, 16, 20, 79]);
    assert_eq!(int_values(&a), vec![0, 2, 4, 6, 8, 10, 99]);
}

/// Unset keeps the next free key, even when the array becomes empty.
#[test]
fn next_free_survives_unset() {
    let mut a = PhpArray::new();
    a.insert(k(5), vi(1));
    a.remove(&k(5));
    assert_eq!(a.next_free_index(), Some(6));
    assert_eq!(a.remove(&k(6)), None);
    a.push(vi(9)).unwrap();
    assert_eq!(a.get(&k(6)), Some(&vi(9)));
}

/// Negative, normalised and extreme integer keys all move the next free key.
#[test]
fn next_free_exact_php_rules() {
    // [5 => 1, 2 => 2] then $a[] goes to 6.
    let mut a = PhpArray::from_iter([(k(5), vi(1)), (k(2), vi(2))]);
    assert_eq!(a.push(vi(3)).unwrap(), k(6));
    // [-5 => 1] then $a[] goes to -4 (PHP 8.3 and later).
    let mut a = PhpArray::new();
    a.insert(k(-5), vi(1));
    assert_eq!(a.push(vi(2)).unwrap(), k(-4));
    // [-5 => 1, 3 => 3] then $a[] goes to 4.
    a.insert(k(3), vi(3));
    assert_eq!(a.push(vi(4)).unwrap(), k(4));
    // ["5" => 1, "6" => 2] then $a[] goes to 7.
    let mut a = PhpArray::new();
    a.insert(ks("5"), vi(1));
    a.insert(ks("6"), vi(2));
    assert_eq!(a.push(vi(3)).unwrap(), k(7));
    // [PHP_INT_MAX => 1] then $a[] is an error.
    let mut a = PhpArray::new();
    a.insert(k(i64::MAX), vi(1));
    let e = a.push(vi(2)).unwrap_err();
    assert_eq!(
        e.message,
        "Cannot add element to the array as the next element is already occupied"
    );
    // [PHP_INT_MIN => 1] then $a[] goes to PHP_INT_MIN + 1.
    let mut a = PhpArray::new();
    a.insert(k(i64::MIN), vi(1));
    assert_eq!(a.push(vi(2)).unwrap(), k(i64::MIN + 1));
    // [1 => 1], unset, then $a[] goes to 2; ['x' => 1], unset, then to 0.
    let mut a = PhpArray::from_iter([(k(1), vi(1))]);
    a.remove(&k(1));
    assert_eq!(a.push(vi(1)).unwrap(), k(2));
    let mut a = PhpArray::from_iter([(ks("x"), vi(1))]);
    a.remove(&ks("x"));
    assert_eq!(a.push(vi(1)).unwrap(), k(0));
    // Pop gives its key back: [5 => 1], pop, then $a[] goes to 5 again.
    let mut a = PhpArray::from_iter([(k(5), vi(1))]);
    a.pop();
    assert_eq!(a.push(vi(1)).unwrap(), k(5));
    // Two pops of [5 => 1, 6 => 2] give back both keys.
    let mut a = PhpArray::from_iter([(k(5), vi(1)), (k(6), vi(2))]);
    a.pop();
    a.pop();
    assert_eq!(a.push(vi(1)).unwrap(), k(5));
}

/// The next free key saturates at PHP_INT_MAX instead of wrapping to negative.
#[test]
fn push_overflow_is_an_error() {
    let mut a = PhpArray::with_capacity(2);
    a.insert(k(i64::MAX), vi(1));
    assert_eq!(a.next_free_index(), Some(i64::MAX));
    assert_eq!(
        a.push(vi(2)).unwrap_err().message,
        "Cannot add element to the array as the next element is already occupied"
    );
}

/// The next free key stops at PHP_INT_MAX, and `$a[]` may still use it once.
#[test]
fn push_may_use_php_int_max_once() {
    // $a = [PHP_INT_MAX - 1 => 1]; $a[] = 2; gives key PHP_INT_MAX.
    let mut a = PhpArray::from_iter([(k(i64::MAX - 1), vi(1))]);
    assert_eq!(a.push(vi(2)).unwrap(), k(i64::MAX));
    assert!(a.push(vi(3)).is_err());
    // After unset($b[PHP_INT_MAX]) the key is free again.
    let mut b = PhpArray::from_iter([(k(i64::MAX), vi(1))]);
    assert!(b.push(vi(2)).is_err());
    b.remove(&k(i64::MAX));
    assert_eq!(b.push(vi(2)).unwrap(), k(i64::MAX));
}

/// Shift renumbers the integer keys that remain; pop keeps every key.
#[test]
fn shift_and_pop_reindexing() {
    // [10 => 1, 'x' => 2, 5 => 3], shift, gives ['x' => 2, 0 => 3].
    let mut a = PhpArray::from_iter([(k(10), vi(1)), (ks("x"), vi(2)), (k(5), vi(3))]);
    assert_eq!(a.shift(), Some(vi(1)));
    assert_eq!(
        a.iter().collect::<Vec<_>>(),
        vec![(ks("x"), &vi(2)), (k(0), &vi(3))]
    );
    assert_eq!(a.next_free_index(), Some(1));
    // ['x' => 1, 5 => 2, 'y' => 3], shift, gives [0 => 2, 'y' => 3].
    let mut a = PhpArray::from_iter([(ks("x"), vi(1)), (k(5), vi(2)), (ks("y"), vi(3))]);
    a.shift();
    assert_eq!(a.keys().collect::<Vec<_>>(), vec![k(0), ks("y")]);
    // Pop does not renumber.
    let mut a = PhpArray::from_iter([(k(10), vi(1)), (ks("x"), vi(2)), (k(5), vi(3))]);
    a.pop();
    assert_eq!(a.keys().collect::<Vec<_>>(), vec![k(10), ks("x")]);
}

/// A foreach position stays valid when an earlier or the current entry is unset.
#[test]
fn positional_iteration_under_modification() {
    let mut a = PhpArray::from_iter([(k(0), vi(10)), (k(1), vi(20)), (k(2), vi(30))]);
    let mut pos = a.first_position().unwrap();
    if let Some((_, v)) = a.entry_at_mut(pos) {
        *v = vi(11);
    }
    pos = a.next_position(pos).unwrap();
    assert_eq!(a.entry_at(pos), Some((k(1), &vi(20))));
    a.remove(&k(1));
    // The hole is skipped.
    pos = a.next_position(pos).unwrap();
    assert_eq!(a.entry_at(pos), Some((k(2), &vi(30))));
    assert_eq!(a.next_position(pos), None);
    assert_eq!(a.first(), Some((k(0), &vi(11))));
    assert_eq!(a.last(), Some((k(2), &vi(30))));
}

/// current(), next(), prev(), reset() and end() move a cursor of their own.
#[test]
fn internal_pointer() {
    let mut a = PhpArray::from_iter([(ks("a"), vi(1)), (ks("b"), vi(2)), (ks("c"), vi(3))]);
    // A new array points at its first entry: current(['a' => 1, ...]) is 1.
    assert_eq!(a.current(), Some(&vi(1)));
    assert_eq!(a.key(), Some(ks("a")));
    a.advance();
    assert_eq!(a.current(), Some(&vi(2)));
    a.end();
    assert_eq!(a.current(), Some(&vi(3)));
    a.rewind_back();
    assert_eq!(a.current(), Some(&vi(2)));
    a.rewind_back();
    a.rewind_back(); // before the first entry: past the end
    assert_eq!(a.current(), None);
    a.advance(); // and next() does not start over
    assert_eq!(a.current(), None);
    a.reset();
    assert_eq!(a.current(), Some(&vi(1)));
}

/// Each step checked against PHP 8.5; see the comments for the PHP code.
#[test]
fn internal_pointer_over_holes_and_appends() {
    // $a = [1, 2, 3]; unset($a[0]); current($a) is 2, key($a) is 1.
    let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2)), (k(2), vi(3))]);
    a.remove(&k(0));
    assert_eq!((a.current(), a.key()), (Some(&vi(2)), Some(k(1))));
    // next($a) after that goes to 3: the hole counts as the entry after it.
    a.advance();
    assert_eq!(a.current(), Some(&vi(3)));

    // $a = [1, 2]; next($a); next($a); next($a); prev($a); all past the end.
    let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2))]);
    a.advance();
    a.advance();
    a.advance();
    assert_eq!(a.current(), None);
    a.rewind_back();
    assert_eq!(a.current(), None);
    // $a[] = 3; current($a) is 3: past the end is where the append goes.
    a.push(vi(3)).unwrap();
    assert_eq!(a.current(), Some(&vi(3)));

    // $a = [1, 2, 3]; end($a); unset($a[2]); current($a) is false;
    // $a[] = 9; current($a) is 9.
    let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2)), (k(2), vi(3))]);
    a.end();
    a.remove(&k(2));
    assert_eq!(a.current(), None);
    a.push(vi(9)).unwrap();
    assert_eq!(a.current(), Some(&vi(9)));
}

/// Compaction moves the pointer with the entry it is on.
#[test]
fn internal_pointer_survives_compaction() {
    let mut a: PhpArray = (0..10).map(|i| (k(i), vi(i))).collect();
    a.end();
    a.rewind_back(); // on 8
    for i in 0..8 {
        a.remove(&k(i));
    }
    a.push(vi(10)).unwrap(); // 8 holes, 3 live: compacts
    assert_eq!(a.storage.len(), 3);
    assert_eq!((a.current(), a.key()), (Some(&vi(8)), Some(k(8))));
}

/// Writing through `Rc::make_mut` copies the array and leaves the original alone.
#[test]
fn copy_on_write_arrays() {
    let original = Rc::new(PhpArray::from_iter([(k(0), vi(1))]));
    let mut copy = Rc::clone(&original);
    Rc::make_mut(&mut copy).insert(k(9), vi(9));
    assert_eq!(original.len(), 1);
    assert_eq!(copy.len(), 2);
}

/// A PHP list has keys 0, 1, 2, ... in order, whatever the storage shape.
#[test]
fn is_list() {
    let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2))]);
    assert!(a.is_list());
    a.insert(k(-1), vi(3));
    assert!(!a.is_list());
    let mut a = PhpArray::new();
    assert!(a.is_list());
    a.insert(ks("z"), vi(1));
    assert!(!a.is_list());
}

/// `iter_mut` changes values in place and keeps their keys.
#[test]
fn iter_mut_changes_values_in_place() {
    let mut a = PhpArray::from_iter([(k(0), vi(1)), (k(1), vi(2))]);
    for (_, v) in a.iter_mut() {
        if let Value::Int(i) = v {
            *i *= 10;
        }
    }
    assert_eq!(a.get(&k(1)), Some(&vi(20)));
    assert!(a.contains_key(&k(0)));
    assert!(!a.contains_key(&k(5)));
    assert_eq!(a.len(), 2);
    assert!(!a.is_empty());
}
