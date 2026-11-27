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
        if matches!(&element, Value::Object(object) if object.lazy().is_some()) {
            return Ok(false);
        }
        self.iter_start_value(iterator, element, place)
    }

    pub(crate) fn iter_start_value(
        &mut self,
        iterator: IterId,
        element: Value,
        place: Option<AccessPath>,
    ) -> Result<bool, NativeError> {
        if let Value::Object(object) = &element
            && object.class.is_traversable()
        {
            return Ok(false);
        }
        let object_snapshot = if let Value::Object(object) = &element {
            if let Some(class) = self.hooked_property_class(object) {
                self.start_hooked_property_iteration(
                    iterator,
                    object.clone(),
                    class,
                    crate::access_mode::Binding::ByValue,
                );
                return Ok(true);
            }
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
                traversal_mode: frame::TraversalMode::Value,
                property_hooks: None,
                source: if object_iteration { None } else { place },
                position: None,
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
                traversal_mode: frame::TraversalMode::Value,
                property_hooks: None,
                source: Some(place),
                position: None,
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
        if state.snapshot.is_none() && (state.traversal.is_some() || state.property_hooks.is_some())
        {
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
    /// Write one iteration entry into its destination. A destination inside
    /// the locals prefix is a named variable, so it is assigned exactly as
    /// `Assign` does: iterators over the old value restart, and a PHP
    /// reference in the slot is written through.
    pub(crate) fn set_iteration_target(
        &mut self,
        register: Reg,
        value: Value,
    ) -> Result<(), NativeError> {
        let locals = self
            .current()
            .map(|frame| frame.function().locals.len())
            .unwrap_or(usize::MAX);
        if register.index() >= locals {
            self.set(register, value);
            return Ok(());
        }
        self.restart_subject_iterators(register);
        self.assign_local(register, value)
    }

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
        // Bind first: the entry's borrow must end before a local destination
        // is assigned through a PHP reference.
        let element_key = match element_key {
            ArrayKey::Int(integer) => Value::Int(integer),
            ArrayKey::Interned(string) => Value::InternedStr(string),
            ArrayKey::String(string) => Value::String(string.clone()),
        };
        let element = element.deref().clone();
        // PHP writes the value, then the key: an old value the loop variable
        // held is released first.
        self.set_iteration_target(destination, element)?;
        if let Some(key) = key {
            self.set_iteration_target(key, element_key)?;
        }
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
        let element_key = match element_key {
            ArrayKey::Int(i) => Value::Int(i),
            ArrayKey::Interned(string) => Value::InternedStr(string),
            ArrayKey::String(string) => Value::String(string),
        };
        let Some(position) = source else {
            // By value, PHP writes the value, then the key.
            self.set_iteration_target(destination, element)?;
            if let Some(key) = key {
                self.set_iteration_target(key, element_key)?;
            }
            return Ok(());
        };
        let key_register = key.unwrap_or(destination);
        self.set_iteration_target(key_register, element_key)?;
        let position = position.clone().with_key(key_register);
        element = self.reference(&position)?;
        self.set_iteration_target(destination, element)?;
        Ok(())
    }
}

/// By-value, unkeyed iteration retains the current
/// register window and local IP. Live reference iteration uses iter_next instead.
///
/// The destination may be the loop's own variable (see the compiler's
/// foreach bindings). A variable that holds a PHP reference is written
/// through by the general handler, so the step leaves before the iterator
/// advances; any other old value is replaced, as `Assign` replaces it.
#[inline(always)]
pub(crate) fn snapshot_step(
    iterator: IterId,
    destination: Reg,
    end: Target,
    iterators: &mut crate::iterators::Iterators,
    slots: &mut [Option<Value>],
    ip: &mut usize,
) -> bool {
    if holds_reference(slots, destination) {
        return false;
    }
    let Some(state) = iterators.get_mut(iterator.0) else {
        return false;
    };
    if state.source.is_some() {
        return false;
    }
    let Some(array) = &state.snapshot else {
        return state.traversal_mode == frame::TraversalMode::Value
            && native_step(state, None, destination, slots);
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
            put(slots, destination, value);
        }
        None => *ip = end.index(),
    }
    true
}

/// `IterNextKeyed` of a by-value loop, as `Vm::iter_next` makes it: the
/// next entry of the array the loop iterates over (its value, then its
/// key, as PHP writes them), or the next of an object's own cursor; `false`
/// with nothing changed when the general handler must run, as for
/// [`snapshot_step`].
#[inline(never)]
pub(crate) fn keyed_step(
    iterator: IterId,
    (key, destination): (Reg, Reg),
    end: Target,
    iterators: &mut crate::iterators::Iterators,
    slots: &mut [Option<Value>],
    ip: &mut usize,
) -> bool {
    if holds_reference(slots, destination) || holds_reference(slots, key) {
        return false;
    }
    let Some(state) = iterators.get_mut(iterator.0) else {
        return false;
    };
    if state.source.is_some() {
        return false;
    }
    let Some(array) = &state.snapshot else {
        return state.traversal_mode == frame::TraversalMode::Value
            && native_step(state, Some(key), destination, slots);
    };
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
        Some((element_key, value)) => {
            let element_key = match element_key {
                ArrayKey::Int(integer) => Value::Int(integer),
                ArrayKey::Interned(string) => Value::InternedStr(string),
                ArrayKey::String(string) => Value::String(string.clone()),
            };
            let value = crate::fast::duplicate(&value.deref());
            put(slots, destination, value);
            put(slots, key, element_key);
        }
        None => *ip = end.index(),
    }
    true
}

pub(crate) enum KeyedAddStep {
    /// Completed or made progress; advance by `spent` work units.
    Done(usize),
    /// Hit work budget mid-loop; yield to scheduler cleanly.
    Yield,
    /// Not applicable (e.g. first pass with missing keys) — run normal `keyed_step`.
    Fallback,
}

/// Bulk keyed-foreach accumulate fast path for:
/// `foreach ($src as $k => $v) { $dst[$k] += $v; }`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn keyed_add_loop_step(
    iterator: IterId,
    (key, value): (Reg, Reg),
    end: Target,
    dst_arr: Reg,
    expr_dst: Reg,
    clear_expr_dst: bool,
    iter_next_ip: usize,
    iterators: &mut crate::iterators::Iterators,
    slots: &mut [Option<Value>],
    ip: &mut usize,
    budget: usize,
) -> KeyedAddStep {
    if dst_arr == key
        || dst_arr == value
        || dst_arr == expr_dst
        || holds_reference(slots, key)
        || holds_reference(slots, value)
        || holds_reference(slots, expr_dst)
    {
        return KeyedAddStep::Fallback;
    }
    let Some(state) = iterators.get_mut(iterator.0) else {
        return KeyedAddStep::Fallback;
    };
    if state.started || state.source.is_some() {
        return KeyedAddStep::Fallback;
    }
    let Some(src_rc) = state.snapshot.clone() else {
        return KeyedAddStep::Fallback;
    };
    let Some(Some(Value::Array(dst_rc))) = slots.get_mut(dst_arr.index()) else {
        return KeyedAddStep::Fallback;
    };
    let outcome =
        std::rc::Rc::make_mut(dst_rc).add_matching_map_entries_from(&src_rc, budget.max(1));
    let write_last = |slots: &mut [Option<Value>], last_pos: usize, last_sum: i64| {
        if let Some((element_key, val)) = src_rc.entry_at(last_pos) {
            let element_key = match element_key {
                ArrayKey::Int(integer) => Value::Int(integer),
                ArrayKey::Interned(string) => Value::InternedStr(string),
                ArrayKey::String(string) => Value::String(string.clone()),
            };
            let val = crate::fast::duplicate(&val.deref());
            put(slots, value, val);
            put(slots, key, element_key);
        }
        if clear_expr_dst {
            if let Some(slot) = slots.get_mut(expr_dst.index()) {
                *slot = None;
            }
        } else {
            put(slots, expr_dst, Value::Int(last_sum));
        }
    };
    match outcome {
        phera_value::MapAddOutcome::Done { last, count } => {
            if let Some((last_pos, last_sum)) = last {
                write_last(slots, last_pos, last_sum);
            }
            state.started = true;
            state.position = None;
            *ip = end.index();
            KeyedAddStep::Done(count)
        }
        phera_value::MapAddOutcome::Partial {
            last: Some((last_pos, last_sum)),
            count,
            yielded,
        } => {
            write_last(slots, last_pos, last_sum);
            state.started = true;
            state.position = Some(last_pos);
            *ip = iter_next_ip;
            if yielded {
                KeyedAddStep::Yield
            } else {
                KeyedAddStep::Done(count)
            }
        }
        phera_value::MapAddOutcome::Partial { last: None, .. } => KeyedAddStep::Fallback,
    }
}

/// Consume a combined entry from a by-value cursor on the ordinary path:
/// the value, then the key.
pub(crate) fn native_step(
    state: &mut frame::IteratorState,
    key: Option<Reg>,
    destination: Reg,
    slots: &mut [Option<Value>],
) -> bool {
    let Some((value, index)) = state
        .traversal
        .as_mut()
        .and_then(|cursor| cursor.next_entry(key.is_some()))
    else {
        return false;
    };
    state.started = true;
    put(slots, destination, owned_value(value));
    if let Some((key, index)) = key.zip(index) {
        put(slots, key, owned_value(index));
    }
    true
}

/// Whether `register` holds a PHP reference, which an iteration entry is
/// written through rather than replacing it.
#[inline(always)]
fn holds_reference(slots: &[Option<Value>], register: Reg) -> bool {
    matches!(slots.get(register.index()), Some(Some(Value::Reference(_))))
}

/// Store one iteration entry over whatever the register held.
#[inline(always)]
fn put(slots: &mut [Option<Value>], register: Reg, value: Value) {
    if let Some(slot) = slots.get_mut(register.index()) {
        *slot = Some(value);
    }
}

fn owned_value(value: Value) -> Value {
    if value.is_reference() {
        value.deref().clone()
    } else {
        value
    }
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
