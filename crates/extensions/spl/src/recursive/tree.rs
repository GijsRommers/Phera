//! RecursiveTreeIterator formats depth and remaining siblings as line prefixes.
//!
//! ```php
//! foreach (new RecursiveTreeIterator($nodes) as $line) echo $line, "\\n";
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PhpString, Value, args,
    traversal::IteratorStep, value::to_bool,
};
use std::rc::Rc;

use super::{
    TREE, at, depth,
    methods::{build, read_current},
    read, uninitialized, update,
};
use crate::{
    This, receiver,
    wrappers::{actual_iterator, cache::stringify},
};

#[derive(Clone)]
pub(super) struct Tree {
    flags: i64,
    parts: [PhpString; 6],
    postfix: PhpString,
}

fn tree(object: &Object) -> Result<Tree, NativeError> {
    read(object, |state| state.tree.clone())?.ok_or_else(uninitialized)
}

pub(super) fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let iterator = args::object(&arguments, 0)
            .ok_or_else(uninitialized)?
            .clone();
        let iterator = actual_iterator(context, iterator).await?;
        let cached = context.create_object("RecursiveCachingIterator", PhpArray::new())?;
        let Value::Object(cached) = cached else {
            return Err(uninitialized());
        };
        let flags = args::int(&arguments, 1).unwrap_or(8);
        let cache_flags = args::int(&arguments, 2).unwrap_or(16);
        let mode = args::int(&arguments, 3).unwrap_or(1);
        context
            .call_method(
                cached.clone(),
                "__construct",
                vec![Value::Object(iterator), Value::Int(cache_flags)],
            )
            .await?;
        let tree = Tree {
            flags,
            parts: ["", "| ", "  ", "|-", "\\-", ""].map(PhpString::from),
            postfix: PhpString::from(""),
        };
        build(context, object, cached, mode, flags, Some(tree)).await
    })
}

async fn prefix(context: &mut dyn Context, object: &Object, tree: &Tree) -> NativeResult {
    let [left, mid_has_next, mid_last, end_has_next, end_last, right] = &tree.parts;
    let depth = depth(object)?;
    let mut bytes = left.as_bytes().to_vec();
    for level in 0..=depth {
        let iterator = at(object, level)?.ok_or_else(uninitialized)?;
        let result = context.call_method(iterator, "hasNext", Vec::new()).await?;
        let part = match (level == depth, to_bool(&result)) {
            (false, true) => mid_has_next,
            (false, false) => mid_last,
            (true, true) => end_has_next,
            (true, false) => end_last,
        };
        bytes.extend_from_slice(part.as_bytes());
    }
    bytes.extend_from_slice(right.as_bytes());
    Ok(Value::from(bytes))
}

async fn entry(context: &mut dyn Context, object: &Object, op: IteratorStep) -> NativeResult {
    let value = read_current(context, object, op).await?;
    if matches!(&*value.deref(), Value::Array(_)) {
        return Ok(Value::from("Array"));
    }
    stringify(context, &value).await
}

fn render<'a>(
    context: &'a mut dyn Context,
    object: Option<Rc<Object>>,
    op: IteratorStep,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = receiver(object.as_ref())?;
        let tree = tree(object)?;
        let bypass = match op {
            IteratorStep::Current => tree.flags & 4 != 0,
            IteratorStep::Key => tree.flags & 8 != 0,
            _ => false,
        };
        if bypass {
            return read_current(context, object, op).await;
        }
        let pre = prefix(context, object, &tree).await?;
        let item = entry(context, object, op).await?;
        let (Value::String(pre), Value::String(item)) = (pre, item) else {
            return Err(NativeError::error("Cannot render recursive iterator entry"));
        };
        let mut text = pre.as_bytes().to_vec();
        text.extend_from_slice(item.as_bytes());
        text.extend_from_slice(tree.postfix.as_bytes());
        Ok(Value::from(text))
    })
}

pub(super) fn current(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    render(context, this, IteratorStep::Current)
}

pub(super) fn key(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    render(context, this, IteratorStep::Key)
}

pub(super) fn get_entry(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { entry(context, receiver(this.as_ref())?, IteratorStep::Current).await })
}

pub(super) fn get_prefix(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        prefix(context, object, &tree(object)?).await
    })
}

pub(super) fn set_part(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let part = args::int(arguments, 0).unwrap_or_default();
    let value = args::string(arguments, 1)
        .ok_or_else(uninitialized)?
        .clone();
    let object = receiver(this)?;
    update(object, |state| {
        let parts = &mut state.tree.as_mut().ok_or_else(uninitialized)?.parts;
        let index = usize::try_from(part).ok();
        let slot = index.and_then(|index| parts.get_mut(index)).ok_or_else(|| {
            NativeError::value_error(format!(
                "{TREE}::setPrefixPart(): Argument #1 ($part) must be a RecursiveTreeIterator::PREFIX_* constant"
            ))
        })?;
        *slot = value;
        Ok(Value::Null)
    })?
}

pub(super) fn get_postfix(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::String(tree(receiver(this)?)?.postfix))
}

pub(super) fn set_postfix(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let postfix = args::string(arguments, 0)
        .ok_or_else(uninitialized)?
        .clone();
    update(receiver(this)?, |state| {
        state.tree.as_mut().ok_or_else(uninitialized)?.postfix = postfix;
        Ok(Value::Null)
    })?
}
