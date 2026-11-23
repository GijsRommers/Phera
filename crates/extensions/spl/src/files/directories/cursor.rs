//! Directory iteration can advance an already loaded stream entry without PHP dispatch.
//!
//! ```php
//! foreach (new FilesystemIterator('src') as $pathname => $info) {}
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeIterator, NativeResult, Object, PhpString, Value,
    args, traversal::IteratorStep, value::to_bool,
};
use std::rc::Rc;

use super::{
    Current, Entries, FILESYSTEM, KEY_FILENAME, KEY_MASK, SKIP_DOTS, change, dot, get, information,
    read,
};
use crate::{This, receiver};

pub(in crate::files) fn rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        move_cursor(context, receiver(this.as_ref())?, Move::Rewind).await?;
        Ok(Value::Null)
    })
}

fn rewind_at(object: &Object) -> Result<(), NativeError> {
    change(object, |dir| {
        dir.position = 0;
        if let Entries::List { offset, .. } = &mut dir.entries {
            *offset = 0;
        }
        dir.skip_dots();
    })
}

pub(in crate::files) fn next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        move_cursor(context, receiver(this.as_ref())?, Move::Next).await?;
        Ok(Value::Null)
    })
}

fn advance(object: &Object) -> Result<(), NativeError> {
    change(object, |dir| {
        dir.position = dir.position.saturating_add(1);
        if let Entries::List { offset, .. } = &mut dir.entries {
            *offset = offset.saturating_add(1);
        }
        dir.skip_dots();
    })
}

enum Move {
    Rewind,
    Next,
}

async fn move_cursor(
    context: &mut dyn Context,
    object: &Object,
    operation: Move,
) -> Result<(), NativeError> {
    let source = get(object, |dir| match &dir.entries {
        Entries::Wrapped { handle, .. } => Some((handle.clone(), dir.flags)),
        Entries::List { .. } => None,
    })?;
    let Some((handle, flags)) = source else {
        return match operation {
            Move::Rewind => rewind_at(object),
            Move::Next => advance(object),
        };
    };
    let method = match operation {
        Move::Rewind => {
            handle.rewind(context).await?;
            "DirectoryIterator::rewind"
        }
        Move::Next => "DirectoryIterator::next",
    };
    let entry = read_wrapper_entry(context, &handle, flags, method).await?;
    change(object, |dir| {
        if let Entries::Wrapped { entry: current, .. } = &mut dir.entries {
            *current = entry;
        }
        dir.position = match operation {
            Move::Rewind => 0,
            Move::Next => dir.position.saturating_add(1),
        };
    })
}

pub(super) async fn read_wrapper_entry(
    context: &mut dyn Context,
    handle: &phera_ext_streams::native::DirectoryStream,
    flags: i64,
    method: &str,
) -> Result<Option<PhpString>, NativeError> {
    while let Some(entry) = handle.read(context, method).await?.to_php_string() {
        if flags & SKIP_DOTS == 0 || !dot(entry.as_bytes()) {
            return Ok(Some(entry));
        }
    }
    Ok(None)
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
                move_cursor(context, object, Move::Rewind).await?;
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
                move_cursor(context, object, Move::Next).await?;
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
    wrapped: bool,
}

pub(in crate::files) fn create(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    let wrapped = get(&object, |dir| {
        matches!(dir.entries, Entries::Wrapped { .. })
    })?;
    let filesystem = object.class.is_instance_of(FILESYSTEM);
    Ok(Box::new(Cursor {
        object,
        filesystem,
        wrapped,
    }))
}

impl NativeIterator for Cursor {
    fn needs_input(&self, operation: IteratorStep) -> bool {
        self.wrapped && matches!(operation, IteratorStep::Rewind | IteratorStep::Advance)
    }

    fn read_input<'a>(
        &'a mut self,
        context: &'a mut dyn Context,
        operation: IteratorStep,
    ) -> LocalFuture<'a, Result<(), NativeError>> {
        let operation = if matches!(operation, IteratorStep::Rewind) {
            Move::Rewind
        } else {
            Move::Next
        };
        Box::pin(move_cursor(context, &self.object, operation))
    }

    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        if self.wrapped {
            Ok(())
        } else {
            rewind_at(&self.object)
        }
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        if self.wrapped {
            Ok(())
        } else {
            advance(&self.object)
        }
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
