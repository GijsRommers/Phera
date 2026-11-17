//! Existing mutation and pointer contracts.

use super::{ArrayKey, PhpArray};
use crate::string::PhpString;
use crate::value::Value;
use std::rc::Rc;

fn k(i: i64) -> ArrayKey {
    ArrayKey::Int(i)
}
fn ks(s: &str) -> ArrayKey {
    ArrayKey::from_value(&Value::from(s)).unwrap()
}
fn vi(i: i64) -> Value {
    Value::Int(i)
}

#[test]
fn order_preserved_after_removals_and_compaction() {
    let mut a = PhpArray::new();
    for i in 0..40 {
        a.insert(k(i * 2), vi(i));
    }
    // Remove every even-indexed key to create many tombstones.
    for i in 0..40 {
        if i % 2 == 1 {
            let removed = a.remove(&k(i * 2));
            assert!(removed.is_some());
        }
    }
    assert_eq!(a.len(), 20);
    let keys: Vec<i64> = a
        .keys()
        .filter_map(|kk| {
            if let ArrayKey::Int(i) = kk {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        keys,
        (0..40)
            .filter(|i| i % 2 == 0)
            .map(|i| i * 2)
            .collect::<Vec<_>>()
    );
    // Keep removing until compaction definitely kicks in (dead > live).
    for i in 0..40 {
        if i % 2 == 0 && i > 10 {
            a.remove(&k(i * 2));
        }
    }
    assert_eq!(a.len(), 6);
    let vals: Vec<i64> = a
        .values()
        .filter_map(|v| {
            if let Value::Int(i) = v {
                Some(*i)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(vals, vec![0, 2, 4, 6, 8, 10]);
}

#[test]
fn next_free_rules() {
    // Removals do not rewind the next-free index...
    let mut a = PhpArray::new();
    a.insert(k(5), vi(1));
    a.remove(&k(5));
    assert_eq!(a.next_free_index(), Some(6));
    // ...except through pop's roll-back rule (covered below); and an
    // emptied array keeps counting from the largest key ever seen.
    let _ = a.remove(&k(6));
    a.push(vi(9)).unwrap();
    assert_eq!(a.get(&k(6)), Some(&vi(9)));
}

#[test]
fn next_free_exact_php_rules() {
    // [5=>'x', 2=>'y'] push -> 6
    let mut a = PhpArray::from_iter([(k(5), vi(1)), (k(2), vi(2))]);
    assert_eq!(a.push(vi(3)).unwrap(), k(6));
    // [-5] push -> -4
    let mut a = PhpArray::new();
    a.insert(k(-5), vi(1));
    assert_eq!(a.push(vi(2)).unwrap(), k(-4));
    // [-5, 3] push -> 4
    a.insert(k(3), vi(3));
    assert_eq!(a.push(vi(4)).unwrap(), k(4));
    // ["5","6"] push -> 7
    let mut a = PhpArray::new();
    a.insert(ks("5"), vi(1));
    a.insert(ks("6"), vi(2));
    assert_eq!(a.push(vi(3)).unwrap(), k(7));
    // [i64::MAX] push -> error
    let mut a = PhpArray::new();
    a.insert(k(i64::MAX), vi(1));
    let e = a.push(vi(2)).unwrap_err();
    assert_eq!(
        e.message,
        "Cannot add element to the array as the next element is already occupied"
    );
    // [i64::MIN] push -> i64::MIN + 1
    let mut a = PhpArray::new();
    a.insert(k(i64::MIN), vi(1));
    assert_eq!(a.push(vi(2)).unwrap(), k(i64::MIN + 1));
    // [1=>'a'] unset -> push -> 2 ; ['x'] unset -> push -> 0
    let mut a = PhpArray::from_iter([(k(1), vi(1))]);
    a.remove(&k(1));
    assert_eq!(a.push(vi(1)).unwrap(), k(2));
    let mut a = PhpArray::from_iter([(ks("x"), vi(1))]);
    a.remove(&ks("x"));
    assert_eq!(a.push(vi(1)).unwrap(), k(0));
    // pop resets to 0 on a fresh-int array: [5] pop -> push 5
    let mut a = PhpArray::from_iter([(k(5), vi(1))]);
    a.pop();
    assert_eq!(a.push(vi(1)).unwrap(), k(5));
    // pop rolls back: [5, 6] two pops -> push 5
    let mut a = PhpArray::from_iter([(k(5), vi(1)), (k(6), vi(2))]);
    a.pop();
    a.pop();
    assert_eq!(a.push(vi(1)).unwrap(), k(5));
}

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

#[test]
fn shift_and_pop_reindexing() {
    // [10'a', 'x''b', 5'c'] shift -> x'b', 0'c'
    let mut a = PhpArray::from_iter([(k(10), vi(1)), (ks("x"), vi(2)), (k(5), vi(3))]);
    let shifted = a.shift().unwrap();
    assert_eq!(shifted, vi(1));
    let got: Vec<(ArrayKey, i64)> = a
        .iter()
        .map(|(kk, v)| {
            (
                kk.clone(),
                if let Value::Int(i) = v { *i } else { panic!() },
            )
        })
        .collect();
    assert_eq!(got, vec![(ks("x"), 2), (k(0), 3)]);
    assert_eq!(a.next_free_index(), Some(1));
    // ['x''a', 5'b', 'y''c'] shift -> 0'b', 'y''c'
    let mut a = PhpArray::from_iter([(ks("x"), vi(1)), (k(5), vi(2)), (ks("y"), vi(3))]);
    a.shift();
    let got: Vec<ArrayKey> = a.keys().collect();
    assert_eq!(got, vec![k(0), ks("y")]);
    // array_pop does not reindex
    let mut a = PhpArray::from_iter([(k(10), vi(1)), (ks("x"), vi(2)), (k(5), vi(3))]);
    a.pop();
    let got: Vec<ArrayKey> = a.keys().collect();
    assert_eq!(got, vec![k(10), ks("x")]);
}

#[test]
fn positional_iteration_under_modification() {
    let mut a = PhpArray::from_iter([(k(0), vi(10)), (k(1), vi(20)), (k(2), vi(30))]);
    let mut pos = a.first_position().unwrap();
    // Mutate while iterating positions.
    if let Some((_, v)) = a.entry_at_mut(pos) {
        *v = vi(11);
    }
    pos = a.next_position(pos).unwrap();
    assert_eq!(a.entry_at(pos), Some((k(1), &vi(20))));
    a.remove(&k(1));
    // The removed slot is skipped.
    pos = a.next_position(pos).unwrap();
    assert_eq!(a.entry_at(pos), Some((k(2), &vi(30))));
    assert_eq!(a.next_position(pos), None);
    assert_eq!(a.first(), Some((k(0), &vi(11))));
    assert_eq!(a.last(), Some((k(2), &vi(30))));
}

#[test]
fn internal_pointer() {
    let mut a = PhpArray::from_iter([(ks("a"), vi(1)), (ks("b"), vi(2)), (ks("c"), vi(3))]);
    assert_eq!(a.current(), None); // pointer unset until reset
    a.reset();
    assert_eq!(a.current(), Some(&vi(1)));
    assert_eq!(a.key(), Some(&ks("a")).cloned());
    a.advance();
    assert_eq!(a.current(), Some(&vi(2)));
    a.end();
    assert_eq!(a.current(), Some(&vi(3)));
    a.rewind_back();
    assert_eq!(a.current(), Some(&vi(2)));
    a.rewind_back();
    a.rewind_back(); // past the start
    assert_eq!(a.current(), None);
    a.advance();
    assert_eq!(a.current(), Some(&vi(1)));
    a.reset();
    assert_eq!(a.current(), Some(&vi(1)));
}

#[test]
fn copy_on_write_arrays() {
    use crate::value::Value;
    let shared = Rc::new(PhpArray::from_iter([(k(0), vi(1))]));
    let a = Value::Array(shared.clone());
    let b = a.clone();
    // Mutate through one handle.
    let Value::Array(r) = &a else { panic!() };
    let mut php_array = (**r).clone();
    php_array.push(vi(2)).unwrap();
    // Proper COW goes through Rc::make_mut on the Rc:
    let Value::Array(r2) = &b else { panic!() };
    let mut cow = r2.clone();
    Rc::make_mut(&mut cow).insert(k(9), vi(9));
    assert_eq!((**r2).len(), 1);
    assert_eq!(cow.len(), 2);
    assert_eq!((**r).len(), 1);
}

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

#[test]
fn mut_iterate_and_defaults() {
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
    let _ = PhpString::default();
}
