//! Walk arrays without Rust recursion, including references: `mb_check_encoding([$value])`.
use phera_extension_api::{ArrayKey, Value};
use std::{collections::HashSet, rc::Rc};
enum Step {
    Visit(Value),
    Leave(usize),
}
pub fn strings(value: Value, keys: bool) -> Result<Vec<Vec<u8>>, ()> {
    let mut pending = vec![Step::Visit(value)];
    let mut active = HashSet::new();
    let mut strings = Vec::new();
    while let Some(step) = pending.pop() {
        let value = match step {
            Step::Leave(id) => {
                active.remove(&id);
                continue;
            }
            Step::Visit(value) => value,
        };
        match value {
            Value::String(s) => strings.push(s.as_bytes().to_vec()),
            Value::Reference(cell) => {
                let id = Rc::as_ptr(&cell) as usize;
                if !active.insert(id) {
                    return Err(());
                }
                pending.push(Step::Leave(id));
                pending.push(Step::Visit(cell.borrow().clone()));
            }
            Value::Array(array) => {
                let id = Rc::as_ptr(&array) as usize;
                if !active.insert(id) {
                    return Err(());
                }
                pending.push(Step::Leave(id));
                for (key, value) in array.iter() {
                    if keys && let ArrayKey::String(key) = key {
                        strings.push(key.as_bytes().to_vec());
                    }
                    pending.push(Step::Visit(value.clone()));
                }
            }
            _ => {}
        }
    }
    Ok(strings)
}
