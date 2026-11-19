//! An extension can expose its cursor as a normal PHP Iterator.
//!
//! ```php
//! $cursor = $period->getIterator();
//! $cursor->valid(); // rewinds on first use
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, LocalFuture, MethodEntry, NativeClone, NativeError,
    NativeIterator, NativeResult, Object, Value, Visibility, traversal::IteratorStep,
};

use crate::traversal::step_native_cursor;

pub(crate) const ENTRY: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    interfaces: &["Iterator"],
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Forbidden),
        ..ClassHooks::EMPTY
    }),
    methods: &[
        MethodEntry::public("__construct", &[], construct).with_visibility(Visibility::Private),
        MethodEntry::public_async("current", &[], current),
        MethodEntry::public_async("key", &[], key),
        MethodEntry::public_async("valid", &[], valid),
        MethodEntry::public_async("next", &[], next),
        MethodEntry::public_async("rewind", &[], rewind),
    ],
    ..ClassEntry::class("InternalIterator")
};

pub(crate) struct InternalIterator {
    pub(crate) cursor: Option<Box<dyn NativeIterator>>,
    pub(crate) initialized: bool,
}

fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot manually construct InternalIterator",
    ))
}

struct ActiveCursor {
    object: Rc<Object>,
    cursor: Option<Box<dyn NativeIterator>>,
}

impl Drop for ActiveCursor {
    fn drop(&mut self) {
        if let Some(state) = self
            .object
            .native
            .borrow_mut()
            .as_mut()
            .and_then(|state| state.downcast_mut::<InternalIterator>())
        {
            state.cursor = self.cursor.take();
        }
    }
}

async fn step(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    operation: IteratorStep,
) -> NativeResult {
    let incomplete =
        || NativeError::error("The InternalIterator object has not been properly initialized");
    let reentered = || NativeError::error("Cannot re-enter an InternalIterator cursor");
    let object = this.ok_or_else(incomplete)?;
    let (cursor, first) = {
        let mut storage = object.native.try_borrow_mut().map_err(|_| reentered())?;
        let state = storage
            .as_mut()
            .and_then(|state| state.downcast_mut::<InternalIterator>())
            .ok_or_else(incomplete)?;
        let cursor = state.cursor.take().ok_or_else(reentered)?;
        let first = !std::mem::replace(&mut state.initialized, true);
        (cursor, first)
    };
    let mut active = ActiveCursor {
        object,
        cursor: Some(cursor),
    };
    let cursor = active.cursor.as_deref_mut().ok_or_else(incomplete)?;
    if first && !matches!(operation, IteratorStep::Rewind) {
        step_native_cursor(cursor, context, IteratorStep::Rewind).await?;
    }
    step_native_cursor(cursor, context, operation).await
}

fn current(
    cx: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(step(cx, this, IteratorStep::Current))
}

fn key(
    cx: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(step(cx, this, IteratorStep::Key))
}

fn valid(
    cx: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(step(cx, this, IteratorStep::Valid))
}

fn next(
    cx: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(step(cx, this, IteratorStep::Advance))
}

fn rewind(
    cx: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(step(cx, this, IteratorStep::Rewind))
}
