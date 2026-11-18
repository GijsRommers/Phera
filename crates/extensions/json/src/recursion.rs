//! A serializer can call json_encode again. The same object remains active.
//!
//! ```php
//! public function jsonSerialize(): mixed { return json_encode($this); }
//! ```

use std::{cell::RefCell, collections::HashSet, rc::Rc};

use phera_extension_api::{Context, Object};

#[derive(Default)]
struct Active(Rc<RefCell<HashSet<usize>>>);

pub(crate) struct Serializing {
    set: Rc<RefCell<HashSet<usize>>>,
    object: Rc<Object>,
}

impl Serializing {
    pub(crate) fn start(context: &mut dyn Context, object: Rc<Object>) -> Option<Self> {
        let set = context.state().get_or_default::<Active>().0.clone();
        if !set.borrow_mut().insert(Rc::as_ptr(&object) as usize) {
            return None;
        }
        Some(Self { set, object })
    }
}

impl Drop for Serializing {
    fn drop(&mut self) {
        self.set
            .borrow_mut()
            .remove(&(Rc::as_ptr(&self.object) as usize));
    }
}
