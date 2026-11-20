//! RecursiveDirectoryIterator opens children through the parent class's constructor.
//!
//! ```php
//! $directory = new RecursiveDirectoryIterator('src', FilesystemIterator::SKIP_DOTS);
//! foreach (new RecursiveIteratorIterator($directory) as $file) {}
//! ```

use phera_ext_streams::native::FileStatus;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args, value::to_bool,
};
use std::rc::Rc;

use super::{FOLLOW_LINKS, change, dot, get, join, read, update};
use crate::{This, receiver};

pub(in crate::files) fn has_children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (directory, location, follow) = get(receiver(this.as_ref())?, |dir| {
            (
                !dir.filename().is_empty() && !dot(dir.filename()),
                dir.location(),
                dir.flags & FOLLOW_LINKS != 0,
            )
        })?;
        if !directory {
            return Ok(Value::Bool(false));
        }
        let pathname = location.pathname.as_bytes();
        let method = "RecursiveDirectoryIterator::hasChildren";
        if !follow
            && !args::bool(&arguments, 0).unwrap_or_default()
            && to_bool(
                &FileStatus::IsLink
                    .get_async(context, pathname, method)
                    .await?,
            )
        {
            return Ok(Value::Bool(false));
        }
        FileStatus::IsDirectory
            .get_async(context, pathname, method)
            .await
    })
}

pub(in crate::files) fn children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (path, flags, sub_path) = get(object, |dir| {
            (
                dir.location().pathname,
                dir.flags,
                join(dir.sub_path.as_bytes(), dir.filename()),
            )
        })?;
        let classes = read(object, |state| state.classes.clone())?;
        let child = context.create_object(object.class.name(), PhpArray::new())?;
        let Value::Object(inner) = &child else {
            return Err(NativeError::error(
                "Directory factory must return an object",
            ));
        };
        context
            .call_method(
                inner.clone(),
                "__construct",
                vec![Value::String(path), Value::Int(flags)],
            )
            .await?;
        // A subclass may skip its parent constructor and retain an invalid cursor.
        update(inner, |state| state.classes = classes)?;
        if read(inner, |state| {
            matches!(state.data, super::Data::Directory(_))
        })? {
            change(inner, |dir| dir.sub_path = sub_path)?;
        }
        Ok(child)
    })
}

pub(in crate::files) fn sub_path(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |dir| Value::String(dir.sub_path.clone()))
}

pub(in crate::files) fn sub_pathname(
    _: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    get(receiver(this)?, |dir| {
        Value::String(join(dir.sub_path.as_bytes(), dir.filename()))
    })
}
