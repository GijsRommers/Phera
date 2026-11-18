//! Foreach opens a cursor, then advances it only after the loop body ran.
//!
//! ```php
//! foreach ($iterator as $value) { break; } // never calls next() or key()
//! ```

use phera_bytecode::{IterId, Reg, Target};
use phera_extension_api::{
    NativeError,
    traversal::{self, IteratorStep},
};
use phera_value::{Value, to_bool};

use crate::{Vm, calls::frame::IteratorState};

impl Vm {
    pub(crate) async fn start_traversable(
        &mut self,
        iterator: IterId,
        subject: Value,
        by_reference: bool,
    ) -> Result<(), NativeError> {
        let Value::Object(object) = subject else {
            return Err(internal_error!("traversal subject"));
        };
        let mut cursor = self.make_traversal(object, by_reference).await?;
        traversal::step(cursor.as_mut(), self, IteratorStep::Rewind).await?;
        let valid = to_bool(&traversal::step(cursor.as_mut(), self, IteratorStep::Valid).await?);
        self.call_stack.current_extras_mut().iterators.insert(
            iterator.0,
            IteratorState {
                snapshot: None,
                source: None,
                traversal: valid.then_some(cursor),
                traversal_by_reference: by_reference,
                position: None,
                started: false,
            },
        );
        Ok(())
    }

    pub(crate) async fn next_traversable(
        &mut self,
        iterator: IterId,
        key: Option<Reg>,
        destination: Reg,
        end: Target,
    ) -> Result<(), NativeError> {
        let mut state = self
            .call_stack
            .current_extras_mut()
            .iterators
            .remove(&iterator.0)
            .ok_or_else(|| internal_error!("missing Traversable iterator"))?;
        let cursor = state
            .traversal
            .as_deref_mut()
            .ok_or_else(|| internal_error!("missing Traversable cursor"))?;
        if state.started {
            traversal::step(cursor, self, IteratorStep::Advance).await?;
            if !to_bool(&traversal::step(cursor, self, IteratorStep::Valid).await?) {
                return self.jump(end);
            }
        }
        let (value, index) = traversal::current(cursor, self, key.is_some()).await?;
        if let Some((key, index)) = key.zip(index) {
            self.set(key, index.deref().clone());
        }
        if state.traversal_by_reference {
            if !value.is_reference() {
                return Err(crate::traversal::reference_error());
            }
            self.set(destination, value);
        } else {
            self.set(destination, value.deref().clone());
        }
        state.started = true;
        self.call_stack
            .current_extras_mut()
            .iterators
            .insert(iterator.0, state);
        Ok(())
    }
}
