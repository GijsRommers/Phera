//! The compiled patterns of a worker thread, so that `preg_match('/a/', $s)`
//! in a loop compiles `/a/` once.
//!
//! Phera runs one single-threaded runtime per core, so each worker thread
//! has its own cache, without locks, shared by the requests it serves. As
//! in PHP, it holds 4096 patterns and drops the oldest eighth when full.

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
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The compiled pattern, or `None` after PHP's warning for a pattern that
/// does not compile (which also sets `PREG_INTERNAL_ERROR`).
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

/// The compiled pattern, or the text of the warning.
pub(crate) fn lookup(pattern: &[u8]) -> Result<Rc<Compiled>, String> {
    let cached = CACHE.with(|cache| cache.borrow().patterns.get(pattern).cloned());
    if let Some(compiled) = cached {
        return Ok(compiled);
    }
    let compiled = Rc::new(compile::compile(pattern)?);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.patterns.len() == CAPACITY {
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
