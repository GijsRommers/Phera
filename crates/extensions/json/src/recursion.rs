//! A serializer can call json_encode again. The same object remains active.
//!
//! ```php
//! public function jsonSerialize(): mixed { return json_encode($this); }
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{Context, Object};

#[derive(Default)]
pub(crate) struct Active(Rc<RefCell<Vec<usize>>>);

pub(crate) struct Serializing {
    active: Rc<RefCell<Vec<usize>>>,
    object: Rc<Object>,
}

impl Serializing {
    pub(crate) fn start(context: &mut dyn Context, object: Rc<Object>) -> Option<Self> {
        let active = context.state().get_or_default::<Active>().0.clone();
        let identity = Rc::as_ptr(&object) as usize;
        {
            let mut stack = active.borrow_mut();
            if stack.contains(&identity) {
                return None;
            }
            stack.push(identity);
        }
        Some(Self { active, object })
    }
}

impl Drop for Serializing {
    fn drop(&mut self) {
        let identity = Rc::as_ptr(&self.object) as usize;
        let mut stack = self.active.borrow_mut();
        if stack.last() == Some(&identity) {
            stack.pop();
        } else if let Some(position) = stack.iter().rposition(|&id| id == identity) {
            stack.swap_remove(position);
        }
    }
}
