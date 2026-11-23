//! Collecting the strings inside nested arrays, as `mb_check_encoding` and
//! `mb_convert_variables` need them:
//!
//! ```php
//! mb_check_encoding(['ok', ["\xFF"]]);  // false: the nested string is invalid
//! $a = [1]; $a[] = &$a;
//! mb_convert_variables('UTF-8', 'ASCII', $a);  // Warning: Cannot handle recursive references
//! ```
//!
//! The walk keeps its own stack instead of recursing, so the depth of PHP
//! data cannot overflow Rust's stack.

use std::collections::HashSet;
use std::rc::Rc;

use phera_extension_api::{ArrayKey, PhpString, Value};

#[derive(Default)]
pub struct Strings {
    pub values: Vec<PhpString>,
    /// String keys of arrays, which `mb_check_encoding` checks as well.
    pub keys: Vec<PhpString>,
}

/// An array or reference contains itself.
pub struct Recursive;

enum Step {
    Visit(Value),
    /// Done with the array or reference of this address, which may then
    /// appear again elsewhere.
    Leave(usize),
}

pub fn strings(value: Value) -> Result<Strings, Recursive> {
    let mut pending = vec![Step::Visit(value)];
    // The containers being walked, by address, to notice a cycle.
    let mut open = HashSet::new();
    let mut strings = Strings::default();
    while let Some(step) = pending.pop() {
        let value = match step {
            Step::Leave(address) => {
                open.remove(&address);
                continue;
            }
            Step::Visit(value) => value,
        };
        match value {
            Value::InternedStr(string) => strings.values.push(string.to_php_string()),
            Value::String(string) => strings.values.push(string),
            Value::Reference(cell) => {
                let address = Rc::as_ptr(&cell) as usize;
                if !open.insert(address) {
                    return Err(Recursive);
                }
                pending.push(Step::Leave(address));
                pending.push(Step::Visit(cell.borrow().clone()));
            }
            Value::Array(array) => {
                let address = Rc::as_ptr(&array) as usize;
                if !open.insert(address) {
                    return Err(Recursive);
                }
                pending.push(Step::Leave(address));
                for (key, value) in array.iter() {
                    match key {
                        ArrayKey::Interned(key) => strings.keys.push(key.to_php_string()),
                        ArrayKey::String(key) => strings.keys.push(key.clone()),
                        ArrayKey::Int(_) => {}
                    }
                    pending.push(Step::Visit(value.clone()));
                }
            }
            _ => {}
        }
    }
    Ok(strings)
}
