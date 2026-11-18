//! An extension can expose its cursor as a normal PHP Iterator.
//!
//! ```php
//! $cursor = $period->getIterator();
//! $cursor->valid(); // rewinds on first use
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, MethodEntry, NativeClone, NativeError,
    NativeIterator, NativeResult, Object, Value, Visibility, traversal::IteratorStep,
};

use crate::traversal::native_step;

pub(crate) const ENTRY: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    interfaces: &["Iterator"],
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Forbidden),
        ..ClassHooks::EMPTY
    }),
    methods: &[
        MethodEntry::public("__construct", &[], construct).with_visibility(Visibility::Private),
        MethodEntry::public("current", &[], current),
        MethodEntry::public("key", &[], key),
        MethodEntry::public("valid", &[], valid),
        MethodEntry::public("next", &[], next),
        MethodEntry::public("rewind", &[], rewind),
    ],
    ..ClassEntry::class("InternalIterator")
};

pub(crate) struct InternalIterator {
    pub(crate) cursor: Box<dyn NativeIterator>,
    pub(crate) initialized: bool,
}

fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot manually construct InternalIterator",
    ))
}

fn step(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    operation: IteratorStep,
) -> NativeResult {
    let incomplete =
        || NativeError::error("The InternalIterator object has not been properly initialized");
    let object = this.ok_or_else(incomplete)?;
    let mut storage = object
        .native
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Cannot re-enter an InternalIterator cursor"))?;
    let state = storage
        .as_mut()
        .and_then(|state| state.downcast_mut::<InternalIterator>())
        .ok_or_else(incomplete)?;
    if !state.initialized {
        state.initialized = true;
        if !matches!(operation, IteratorStep::Rewind) {
            state.cursor.rewind(context)?;
        }
    }
    native_step(state.cursor.as_mut(), context, operation)
}

fn current(cx: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    step(cx, this, IteratorStep::Current)
}

fn key(cx: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    step(cx, this, IteratorStep::Key)
}

fn valid(cx: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    step(cx, this, IteratorStep::Valid)
}

fn next(cx: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    step(cx, this, IteratorStep::Advance)
}

fn rewind(cx: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    step(cx, this, IteratorStep::Rewind)
}
