//! Enumerate the live tokens in their attribute order.
//!
//! ```php
//! foreach ($element->classList as $token) echo $token;
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};

use super::{Tokens, state};

struct Cursor {
    source: Tokens,
    revision: Option<u64>,
    tokens: Vec<String>,
    index: usize,
}

impl Cursor {
    fn refresh(&mut self) {
        let revision = self.source.root.shared.borrow().document.arena.revision();
        if self.revision != Some(revision) {
            self.revision = Some(revision);
            self.tokens = self.source.tokens();
        }
    }
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = 0;
        Ok(())
    }
    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = self.index.saturating_add(1);
        Ok(())
    }
    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        self.refresh();
        Ok(self.index < self.tokens.len())
    }
    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        self.refresh();
        Ok(self
            .tokens
            .get(self.index)
            .map_or(Value::Null, |token| Value::from(token.as_str())))
    }
    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(Value::Int(self.index as i64))
    }
}

pub(super) fn iterator(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        source: state(&object)?,
        revision: None,
        tokens: Vec::new(),
        index: 0,
    }))
}
