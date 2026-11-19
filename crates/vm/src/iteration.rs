//! Foreach snapshots and live reference traversal.

use phera_bytecode::{IterId, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};

use crate::{AccessPath, Vm, calls::frame};

impl Vm {
    /// By-value foreach retains an array snapshot, so later writes separate by COW.
    pub(crate) fn iter_start(
        &mut self,
        iterator: IterId,
        source: Reg,
        place: Option<AccessPath>,
    ) -> Result<bool, NativeError> {
        let element = self.get(source);
        if let Value::Object(object) = &element
            && object.class.is_traversable()
        {
            return Ok(false);
        }
        let object_snapshot = if let Value::Object(object) = &element {
            Some(std::rc::Rc::new(self.visible_properties(
                object,
                phera_extension_api::PropertyPurpose::Foreach,
            )?))
        } else {
            None
        };
        let object_iteration = object_snapshot.is_some();
        let snapshot = if object_snapshot.is_some() {
            object_snapshot
        } else if let Value::Array(array) = element {
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
        self.call_stack.current_extras_mut().iterators.insert(
            iterator.0,
            frame::IteratorState {
                snapshot: if object_iteration || place.is_none() {
                    snapshot
                } else {
                    None
                },
                traversal: None,
                source: if object_iteration { None } else { place },
                position: None,
                traversal_reference: false,
                started: false,
            },
        );

        Ok(true)
    }

    /// By-reference foreach follows a live place instead of an array snapshot.
    pub(crate) fn iter_start_reference(
        &mut self,
        iterator: IterId,
        place: AccessPath,
    ) -> Result<(), NativeError> {
        let value = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        if !value.is_array() && !value.is_object() {
            self.warn(format!(
                "foreach() argument must be of type array|object, {} given",
                match value {
                    Value::Bool(true) => "true".into(),
                    Value::Bool(false) => "false".into(),
                    _ => value.type_name(),
                }
            ));
        }
        self.call_stack.current_extras_mut().iterators.insert(
            iterator.0,
            frame::IteratorState {
                snapshot: None,
                traversal: None,
                source: Some(place),
                position: None,
                traversal_reference: false,
                started: false,
            },
        );

        Ok(())
    }

    /// Put a `foreach` state back into the running frame's side table.
    fn keep_iterator(&mut self, iterator: IterId, state: frame::IteratorState) {
        self.call_stack
            .current_extras_mut()
            .iterators
            .insert(iterator.0, state);
    }

    /// Advance in insertion order and bind a live element cell for reference iteration.
    pub(crate) fn iter_next<const TRACK_GENERATORS: bool>(
        &mut self,
        iterator: IterId,
        key: Option<Reg>,
        value: Reg,
        end: Target,
    ) -> Result<bool, NativeError> {
        if TRACK_GENERATORS && self.next_generator_element(iterator, key, value)? {
            return Ok(true);
        }
        let mut state = self
            .call_stack
            .current_extras_mut()
            .iterators
            .remove(&iterator.0)
            .ok_or_else(|| internal_error!("missing iterator"))?;
        if state.snapshot.is_none() && state.traversal.is_some() {
            self.keep_iterator(iterator, state);
            return Ok(false);
        }
        if state.source.is_none() {
            self.next_snapshot(&mut state, key, value, end)?;
            self.keep_iterator(iterator, state);
            return Ok(true);
        }
        let array = if let Some(position) = &state.source {
            match self.read_place(position, crate::access_mode::Missing::Silent)? {
                Value::Array(array) => Some(array),
                Value::Object(object) => {
                    self.next_object_reference(&mut state, &object, key, value, end)?;
                    self.keep_iterator(iterator, state);
                    return Ok(true);
                }
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
            .map(|(element_key, element)| (element_key, element.deref().clone()));
        drop(array);
        if let Some((element_key, element)) = entry {
            self.bind_iteration_entry(element_key, element, state.source.as_ref(), key, value)?;
        } else {
            self.release_iterator_subject(&mut state)?;
            self.jump(end)?;
        }
        self.keep_iterator(iterator, state);
        Ok(true)
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
                    ArrayKey::Int(integer) => Value::Int(integer),
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

/// By-value, unkeyed iteration retains the current
/// register window and local IP. Live reference iteration uses iter_next instead.
#[inline(always)]
pub(crate) fn snapshot_step(
    iterator: IterId,
    destination: Reg,
    end: Target,
    iterators: &mut crate::iterators::Iterators,
    slots: &mut [Option<Value>],
    ip: &mut usize,
) -> bool {
    let Some(state) = iterators.get_mut(iterator.0) else {
        return false;
    };
    if state.source.is_some() {
        return false;
    }
    let Some(array) = &state.snapshot else {
        return false;
    };
    // Plain matches rather than `Option` combinators: this runs on every
    // round of a `foreach`, and a closure is a separate function that the
    // compiler need not inline.
    let position = match (state.started, state.position) {
        (true, Some(position)) => array.next_position(position),
        (true, None) => None,
        (false, _) => array.first_position(),
    };
    state.position = position;
    state.started = true;
    let entry = match position {
        Some(position) => array.entry_at(position),
        None => None,
    };
    match entry {
        Some((_, value)) => {
            let value = crate::fast::duplicate(&value.deref());
            if let Some(slot) = slots.get_mut(destination.index()) {
                *slot = Some(value);
            }
        }
        None => *ip = end.index(),
    }
    true
}

impl Vm {
    /// Exhaustion and break must release the iterator's scratch alias to its subject.
    pub(crate) fn release_iterator_subject(
        &mut self,
        state: &mut frame::IteratorState,
    ) -> Result<(), NativeError> {
        if let Some(subject) = state
            .source
            .take()
            .and_then(|source| source.local_register())
        {
            self.clear_local(subject)?;
        }
        Ok(())
    }
}
