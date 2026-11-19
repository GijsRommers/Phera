//! File information factories accept subclasses and run their PHP constructors.
//!
//! ```php
//! $parent = $info->getPathInfo(MyFileInfo::class);
//! ```

use phera_ext_streams::native;
use phera_extension_api::{
    Context, LocalFuture, MethodPreparation, NativeError, NativeResult, Object, Param, PhpArray,
    Value, args, value::to_php_string,
};
use std::rc::Rc;

use super::{Data, FILE, INFO, Location, location, read, update};
use crate::receiver;

async fn resolve_class(
    context: &mut dyn Context,
    name: &[u8],
    base: &str,
    method: &str,
    nullable: bool,
) -> Result<Rc<str>, NativeError> {
    let class = match std::str::from_utf8(name) {
        Ok(name) => context.lookup_class(name).await?,
        Err(_) => None,
    };
    if let Some(class) = class.as_ref().filter(|class| class.is_instance_of(base)) {
        return Ok(Rc::from(class.name()));
    }
    let requirement = if method == "getPathInfo" && class.is_none() {
        "a valid class name".to_owned()
    } else {
        format!("a class name derived from {base}")
    };
    let nullable = if nullable { " or null" } else { "" };
    let shown = name.split(|byte| *byte == 0).next().unwrap_or_default();
    let prefix =
        format!("SplFileInfo::{method}(): Argument #1 ($class) must be {requirement}{nullable}, ");
    Err(NativeError::throw_bytes(
        "TypeError",
        [prefix.as_bytes(), shown, b" given"].concat(),
    ))
}

fn set_class<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    file: bool,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (base, method) = if file {
            (FILE, "setFileClass")
        } else {
            (INFO, "setInfoClass")
        };
        let name = if arguments.is_empty() {
            Rc::from(base)
        } else {
            resolve_class(context, args::bytes(&arguments, 0), base, method, false).await?
        };
        update(object, |state| {
            if file {
                state.classes.file = name;
            } else {
                state.classes.info = name;
            }
        })?;
        Ok(Value::Null)
    })
}

pub(super) fn prepare_class_name(
    context: &mut dyn Context,
    params: &[Param],
    arguments: &mut [Value],
) -> Result<MethodPreparation, NativeError> {
    if let Some(value) = arguments.first_mut()
        && !(value.is_null() && params.first().is_some_and(|param| param.nullable))
    {
        *value = Value::String(to_php_string(value, context.diagnostics())?);
    }
    Ok(MethodPreparation::Default)
}

pub(super) fn set_info(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    set_class(context, this, arguments, false)
}

pub(super) fn set_file(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    set_class(context, this, arguments, true)
}

pub(super) fn file_info(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    make_info(context, this, arguments, false)
}

pub(super) fn path_info(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    make_info(context, this, arguments, true)
}

fn make_info(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    parent: bool,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        if read(
            object,
            |state| matches!(&state.data, Data::Directory(dir) if dir.entry().is_none()),
        )? {
            return if parent {
                Ok(Value::Null)
            } else {
                Err(NativeError::throw(
                    "RuntimeException",
                    "Could not open file",
                ))
            };
        }
        let location = location(object)?;
        let name = if parent {
            native::parent(location.pathname.as_bytes())
        } else {
            location.pathname.as_bytes()
        };
        let method = if parent { "getPathInfo" } else { "getFileInfo" };
        let class = if let Some(class) = args::string(&arguments, 0) {
            resolve_class(context, class.as_bytes(), INFO, method, true).await?
        } else {
            read(object, |state| state.classes.info.clone())?
        };
        if parent && name.is_empty() {
            return Ok(Value::Null);
        }
        create_info(context, &class, Location::info(name)).await
    })
}

pub(in crate::files) async fn create_info(
    context: &mut dyn Context,
    class: &str,
    location: Location,
) -> NativeResult {
    let value = context.create_object(class, PhpArray::new())?;
    let Value::Object(object) = &value else {
        return Err(NativeError::error(
            "File information constructor requires an object",
        ));
    };
    if class == INFO {
        update(object, |state| state.data = Data::Info(location))?;
    } else {
        context
            .call_method(
                object.clone(),
                "__construct",
                vec![Value::String(location.pathname)],
            )
            .await?;
    }
    Ok(value)
}
