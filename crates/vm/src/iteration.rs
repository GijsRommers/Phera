//! Foreach snapshots and live reference traversal.

use phera_bytecode::{IterId, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};

use crate::{AccessPath, Vm, frame};

impl Vm {
    /// By-value foreach retains an array snapshot, so later writes separate by COW.
    pub(crate) fn iter_start(
        &mut self,
        iterator: IterId,
        source: Reg,
        place: Option<AccessPath>,
    ) -> Result<(), NativeError> {
        let element = self.get(source);
        let snapshot = if let Value::Array(array) = element {
            Some(array)
        } else {
            self.warn(format!(
                "foreach() argument must be of type array|object, {} given",
                match element {
                    Value::Bool(true) => "true".into(),
                    Value::Bool(false) => "false".into(),
                    _ => element.type_name(),
                }
            ));
            None
        };
        if let Some(frame) = self.call_stack.current_mut() {
            frame.iterators.insert(
                iterator.0,
                frame::IteratorState {
                    snapshot: if place.is_none() { snapshot } else { None },
                    source: place,
                    position: None,
                    started: false,
                },
            );
        }

        Ok(())
    }

    /// By-reference foreach follows a live place instead of an array snapshot.
    pub(crate) fn iter_start_reference(
        &mut self,
        iterator: IterId,
        place: AccessPath,
    ) -> Result<(), NativeError> {
        let value = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        if !value.is_array() {
            self.warn(format!(
                "foreach() argument must be of type array|object, {} given",
                match value {
                    Value::Bool(true) => "true".into(),
                    Value::Bool(false) => "false".into(),
                    _ => value.type_name(),
                }
            ));
        }
        if let Some(frame) = self.call_stack.current_mut() {
            frame.iterators.insert(
                iterator.0,
                frame::IteratorState {
                    snapshot: None,
                    source: Some(place),
                    position: None,
                    started: false,
                },
            );
        }
        Ok(())
    }

    /// Advance in insertion order and bind a live element cell for reference iteration.
    pub(crate) fn iter_next(
        &mut self,
        iterator: IterId,
        key: Option<Reg>,
        value: Reg,
        end: Target,
    ) -> Result<(), NativeError> {
        let mut state = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.iterators.remove(&iterator.0))
            .ok_or_else(|| internal_error!("missing iterator"))?;
        if state.source.is_none() {
            self.next_snapshot(&mut state, key, value, end)?;
            if let Some(frame) = self.call_stack.current_mut() {
                frame.iterators.insert(iterator.0, state);
            }
            return Ok(());
        }
        let array = if let Some(position) = &state.source {
            match self.read_place(position, crate::access_mode::Missing::Silent)? {
                Value::Array(array) => Some(array),
                _ => None,
            }
        } else {
            state.snapshot.clone()
        };
        let position = array.as_ref().and_then(|array| {
            if state.started {
                state
                    .position
                    .and_then(|position| array.next_position(position))
            } else {
                array.first_position()
            }
        });
        state.started = true;
        state.position = position;
        let entry = array
            .as_ref()
            .and_then(|array| position.and_then(|position| array.entry_at(position)))
            .map(|(element_key, element)| (element_key.clone(), element.deref().clone()));
        drop(array);
        if let Some((element_key, element)) = entry {
            self.bind_iteration_entry(element_key, element, state.source.as_ref(), key, value)?;
        } else {
            self.jump(end)?;
        }
        if let Some(frame) = self.call_stack.current_mut() {
            frame.iterators.insert(iterator.0, state);
        }

        Ok(())
    }
}

impl Vm {
    /// Snapshot iteration borrows its owner directly, avoiding a new array owner on every element.
    #[inline]
    fn next_snapshot(
        &mut self,
        state: &mut frame::IteratorState,
        key: Option<Reg>,
        destination: Reg,
        end: Target,
    ) -> Result<(), NativeError> {
        let position = state.snapshot.as_ref().and_then(|array| {
            if state.started {
                state
                    .position
                    .and_then(|position| array.next_position(position))
            } else {
                array.first_position()
            }
        });
        state.started = true;
        state.position = position;
        let Some((element_key, element)) = state
            .snapshot
            .as_ref()
            .and_then(|array| position.and_then(|position| array.entry_at(position)))
        else {
            return self.jump(end);
        };
        if let Some(key) = key {
            self.set(
                key,
                match element_key {
                    ArrayKey::Int(integer) => Value::Int(*integer),
                    ArrayKey::String(string) => Value::String(string.clone()),
                },
            );
        }
        self.set(destination, element.deref().clone());
        Ok(())
    }

    /// Publish the key before resolving a by-reference element path that uses its register.
    fn bind_iteration_entry(
        &mut self,
        element_key: ArrayKey,
        mut element: Value,
        source: Option<&AccessPath>,
        key: Option<Reg>,
        destination: Reg,
    ) -> Result<(), NativeError> {
        if source.is_none() && key.is_none() {
            self.set(destination, element);
            return Ok(());
        }
        let key_register = key.unwrap_or(destination);
        self.set(
            key_register,
            match element_key {
                ArrayKey::Int(i) => Value::Int(i),
                ArrayKey::String(string) => Value::String(string),
            },
        );
        if let Some(position) = source {
            let position = position.clone().with_key(key_register);
            element = self.reference(&position)?;
        }
        self.set(destination, element);
        Ok(())
    }
}
