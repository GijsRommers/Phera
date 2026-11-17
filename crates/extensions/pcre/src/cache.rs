//! Repeated `preg_match('/a/', $s)` calls reuse a worker's compiled pattern.

use crate::compile::{self, Compiled};
use crate::errors::{self, Error};
use phera_extension_api::Context;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

const CAPACITY: usize = 4096;
#[derive(Default)]
struct Cache {
    patterns: HashMap<Vec<u8>, Rc<Compiled>>,
    insertion_order: VecDeque<Vec<u8>>,
}
thread_local! {
    // Phera runs one single-threaded runtime per core. Sharing this cache across
    // requests on that worker avoids locks and never holds a borrow across await.
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}
pub(crate) fn get(
    context: &mut dyn Context,
    function: &str,
    pattern: &[u8],
) -> Option<Rc<Compiled>> {
    match lookup(pattern) {
        Ok(compiled) => Some(compiled),
        Err(message) => {
            context.warning(format!("{function}(): {message}"));
            errors::set(context, Error::Internal);
            None
        }
    }
}

pub(crate) fn lookup(pattern: &[u8]) -> Result<Rc<Compiled>, String> {
    let cached = CACHE.with(|cache| cache.borrow().patterns.get(pattern).cloned());
    if let Some(compiled) = cached {
        return Ok(compiled);
    }
    let compiled = Rc::new(compile::compile(pattern)?);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.patterns.len() == CAPACITY {
            // PHP evicts the oldest eighth when its 4096-entry cache fills.
            for _ in 0..CAPACITY / 8 {
                if let Some(key) = cache.insertion_order.pop_front() {
                    cache.patterns.remove(&key);
                }
            }
        }
        cache.insertion_order.push_back(pattern.to_vec());
        cache.patterns.insert(pattern.to_vec(), compiled.clone());
    });
    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::{CACHE, CAPACITY, lookup};

    #[test]
    fn full_pattern_keys_and_bounded_worker_storage() {
        CACHE.with(|cache| *cache.borrow_mut() = Default::default());
        let plain = lookup(b"/a/").unwrap();
        let cached = lookup(b"/a/").unwrap();
        let insensitive = lookup(b"/a/i").unwrap();
        assert!(std::rc::Rc::ptr_eq(&plain, &cached));
        assert!(!std::rc::Rc::ptr_eq(&plain, &insensitive));
        for index in 0..CAPACITY {
            lookup(format!("/entry{index}/").as_bytes()).unwrap();
        }
        CACHE.with(|cache| assert!(cache.borrow().patterns.len() <= CAPACITY));
        assert!(plain.regex.is_match(b"a").unwrap());
    }
}
