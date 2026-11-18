//! RecursiveIteratorIterator uses a heap-allocated stack of inner iterators.
//!
//! ```php
//! $rows = new RecursiveIteratorIterator(new RecursiveArrayIterator([[1], [2]]));
//! foreach ($rows as $value) echo $value; // 12
//! ```

mod methods;
mod registration;
mod tree;
mod walk;

pub(crate) use registration::register;

use phera_extension_api::{
    Context, NativeError, NativeResult, Object,
    traversal::{self, IteratorStep, Traversal},
};
use std::{collections::HashSet, rc::Rc};

const CLASS: &str = "RecursiveIteratorIterator";
const TREE: &str = "RecursiveTreeIterator";

#[derive(Clone, Copy)]
enum Mode {
    Leaves,
    PreOrder,
    PostOrder,
    Other,
}

impl Mode {
    fn parse(value: i64) -> Self {
        match value {
            0 => Self::Leaves,
            1 => Self::PreOrder,
            2 => Self::PostOrder,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Start,
    Advance,
    Test,
    YieldParent,
    Descend,
    End,
}

struct Level {
    inner: Rc<Object>,
    cursor: Option<Box<dyn Traversal>>,
    phase: Phase,
}

#[derive(Default)]
struct State {
    levels: Vec<Level>,
    active: HashSet<u32>,
    settings: Option<Settings>,
    max_depth: Option<i64>,
    in_iteration: bool,
    running: bool,
    tree: Option<tree::Tree>,
}

#[derive(Clone, Copy)]
struct Settings {
    mode: Mode,
    catch_children: bool,
}

fn read<T>(object: &Object, f: impl FnOnce(&State) -> T) -> Result<T, NativeError> {
    crate::read_state(object, |state: &State| state.settings.map(|_| f(state)))?
        .ok_or_else(|| initialization_error(object))
}

fn update<T>(object: &Object, f: impl FnOnce(&mut State) -> T) -> Result<T, NativeError> {
    crate::update_state(object, |state: &mut State| state.settings.map(|_| f(state)))?
        .ok_or_else(|| initialization_error(object))
}

fn initialization_error(object: &Object) -> NativeError {
    NativeError::error(format!(
        "The {} instance wasn't initialized properly",
        object.class.name()
    ))
}

fn uninitialized() -> NativeError {
    NativeError::error("Object is not initialized")
}

fn depth(object: &Object) -> Result<usize, NativeError> {
    read(object, |state| state.levels.len().saturating_sub(1))
}

fn at(object: &Object, level: usize) -> Result<Option<Rc<Object>>, NativeError> {
    read(object, |state| {
        state.levels.get(level).map(|level| level.inner.clone())
    })
}

fn inner(object: &Object) -> Result<Rc<Object>, NativeError> {
    at(object, depth(object)?)?.ok_or_else(uninitialized)
}

async fn attach(
    context: &mut dyn Context,
    object: &Rc<Object>,
    iterator: Rc<Object>,
) -> Result<(), NativeError> {
    if Rc::ptr_eq(object, &iterator) || read(object, |state| state.active.contains(&iterator.id))? {
        return Err(NativeError::throw(
            "UnexpectedValueException",
            "RecursiveIterator already occurs in the active path",
        ));
    }
    let cursor = context.traverse(iterator.clone()).await?;
    update(object, |state| {
        state.active.insert(iterator.id);
        state.levels.push(Level {
            inner: iterator,
            cursor: Some(cursor),
            phase: Phase::Start,
        });
    })
}

fn detach(object: &Object) -> Result<(), NativeError> {
    update(object, |state| {
        if let Some(level) = state.levels.pop() {
            state.active.remove(&level.inner.id);
        }
    })
}

async fn step(
    context: &mut dyn Context,
    object: &Object,
    level: usize,
    operation: IteratorStep,
) -> NativeResult {
    let mut cursor = update(object, |state| {
        state
            .levels
            .get_mut(level)
            .and_then(|level| level.cursor.take())
    })?
    .ok_or_else(|| NativeError::error("Cannot re-enter a recursive iterator cursor"))?;
    let result = traversal::step(cursor.as_mut(), context, operation).await;
    update(object, |state| {
        if let Some(level) = state.levels.get_mut(level) {
            level.cursor = Some(cursor);
        }
    })?;
    result
}

fn phase(object: &Object, phase: Phase) -> Result<(), NativeError> {
    update(object, |state| {
        state.levels.last_mut().ok_or_else(uninitialized)?.phase = phase;
        Ok(())
    })?
}

async fn hook(
    context: &mut dyn Context,
    object: &Rc<Object>,
    name: &str,
) -> Result<(), NativeError> {
    if context.is_user_method(object, name) {
        context
            .call_method(object.clone(), name, Vec::new())
            .await?;
    }
    Ok(())
}
