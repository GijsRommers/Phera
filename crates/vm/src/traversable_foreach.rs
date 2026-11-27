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

use crate::{
    Vm,
    calls::frame::{IteratorState, TraversalMode},
};

impl Vm {
    /// A resumed generator either yields its next element here or enters the
    /// ordinary cursor path at the first instruction that needs the VM.
    pub(crate) fn next_generator_element(
        &mut self,
        iterator: IterId,
        key: Option<Reg>,
        destination: Reg,
    ) -> Result<bool, NativeError> {
        let generator = self
            .call_stack
            .current_extras_mut()
            .iterators
            .get_mut(iterator.0)
            .filter(|state| state.started && state.traversal_mode == TraversalMode::Generator)
            .and_then(|state| state.traversal.as_deref()?.generator())
            .cloned();
        let Some(generator) = generator else {
            return Ok(false);
        };
        let Some(next) = self.resume_generator_element(&generator, key.is_some()) else {
            return Ok(false);
        };
        let (element, index) = next?;
        if let Some((key, index)) = key.zip(index) {
            self.set(key, index.deref().clone());
        }
        self.set(destination, element);
        Ok(true)
    }

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
        let traversal_mode = if by_reference {
            TraversalMode::Reference
        } else if cursor.generator().is_some() {
            TraversalMode::Generator
        } else {
            TraversalMode::Value
        };
        self.call_stack.current_extras_mut().iterators.insert(
            iterator.0,
            IteratorState {
                snapshot: None,
                source: None,
                traversal: valid.then_some(cursor),
                traversal_mode,
                property_hooks: None,
                position: None,
                started: false,
                bulk_resume: false,
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
        if self
            .next_hooked_property(iterator, key, destination, end)
            .await?
        {
            return Ok(());
        }
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
        if state.traversal_mode == TraversalMode::Reference {
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
