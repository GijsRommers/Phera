//! Directory iteration can advance an already loaded stream entry without PHP dispatch.
//!
//! ```php
//! foreach (new FilesystemIterator('src') as $pathname => $info) {}
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeIterator, NativeResult, Object, Value, args,
    value::to_bool,
};
use std::rc::Rc;

use super::{Current, FILESYSTEM, KEY_FILENAME, KEY_MASK, change, get, information, read};
use crate::{This, receiver};

pub(in crate::files) fn rewind(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    rewind_at(receiver(this)?)?;
    Ok(Value::Null)
}

fn rewind_at(object: &Object) -> Result<(), NativeError> {
    change(object, |dir| {
        dir.position = 0;
        dir.offset = 0;
        dir.skip_dots();
    })
}

pub(in crate::files) fn next(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    advance(receiver(this)?)?;
    Ok(Value::Null)
}

fn advance(object: &Object) -> Result<(), NativeError> {
    change(object, |dir| {
        dir.position = dir.position.saturating_add(1);
        dir.offset = dir.offset.saturating_add(1);
        dir.skip_dots();
    })
}

pub(in crate::files) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |dir| Value::Bool(dir.entry().is_some()))
}

pub(in crate::files) fn index(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |dir| Value::Int(dir.position))
}

pub(in crate::files) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    get(object, |dir| {
        if dir.flags & KEY_MASK == KEY_FILENAME {
            Value::from(dir.filename().to_vec())
        } else {
            Value::String(dir.location().pathname)
        }
    })
}

pub(in crate::files) fn identity(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    get(object, |_| Value::Object(object.clone()))
}

pub(in crate::files) fn current(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (mode, location, valid) = get(object, |dir| {
            (dir.current_mode(), dir.location(), dir.entry().is_some())
        })?;
        match mode {
            Current::Directory => Ok(Value::Object(object.clone())),
            Current::Pathname => Ok(Value::String(location.pathname)),
            Current::FileInfo if valid => {
                let class = read(object, |state| state.classes.info.clone())?;
                information::create_info(context, &class, location).await
            }
            Current::FileInfo => Err(NativeError::throw(
                "RuntimeException",
                "Could not open file",
            )),
        }
    })
}

pub(in crate::files) fn seek(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let target = args::int(&arguments, 0).unwrap_or_default();
        if get(object, |dir| dir.position)? > target {
            if context.is_user_method(object, "rewind") {
                context
                    .call_method(object.clone(), "rewind", Vec::new())
                    .await?;
            } else {
                rewind_at(object)?;
            }
        }
        let custom_valid = context.is_user_method(object, "valid");
        let custom_next = context.is_user_method(object, "next");
        while get(object, |dir| dir.position)? < target {
            let valid = if custom_valid {
                to_bool(
                    &context
                        .call_method(object.clone(), "valid", Vec::new())
                        .await?,
                )
            } else {
                get(object, |dir| dir.entry().is_some())?
            };
            if !valid {
                return Err(NativeError::throw(
                    "OutOfBoundsException",
                    format!("Seek position {target} is out of range"),
                ));
            }
            let before = get(object, |dir| dir.position)?;
            if custom_next {
                context
                    .call_method(object.clone(), "next", Vec::new())
                    .await?;
            } else {
                advance(object)?;
            }
            if get(object, |dir| dir.position)? <= before {
                return Err(NativeError::throw(
                    "OutOfBoundsException",
                    format!("Seek position {target} is out of range"),
                ));
            }
        }
        Ok(Value::Null)
    })
}

struct Cursor {
    object: Rc<Object>,
    filesystem: bool,
}

pub(in crate::files) fn create(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    get(&object, |_| ())?;
    let filesystem = object.class.is_instance_of(FILESYSTEM);
    Ok(Box::new(Cursor { object, filesystem }))
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        rewind_at(&self.object)
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        advance(&self.object)
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        get(&self.object, |dir| dir.entry().is_some())
    }

    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        let (mode, location) = get(&self.object, |dir| (dir.current_mode(), dir.location()))?;
        match mode {
            Current::Directory => Ok(Value::Object(self.object.clone())),
            Current::Pathname => Ok(Value::String(location.pathname)),
            Current::FileInfo => {
                let class = read(&self.object, |state| state.classes.info.clone())?;
                crate::make(
                    context,
                    &class,
                    super::super::State {
                        data: super::Data::Info(location),
                        ..super::super::State::default()
                    },
                )
                .map(Value::Object)
            }
        }
    }

    fn key(&mut self, context: &mut dyn Context) -> NativeResult {
        if self.filesystem {
            key(context, Some(&self.object), &[])
        } else {
            index(context, Some(&self.object), &[])
        }
    }
}
