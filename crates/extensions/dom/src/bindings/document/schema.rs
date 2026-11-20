//! The DOM entry points for validation against a file or an inline schema.
//!
//! ```php
//! $document->schemaValidateSource($schema, LIBXML_SCHEMA_CREATE);
//! ```

use std::path::{Path, PathBuf};
use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, WorkingDirectory, args,
};

use super::super::{libxml, this_node};
use crate::schema::loader::{self, Input};

pub(super) fn file(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(validate(context, this, arguments, true))
}

pub(super) fn source(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(validate(context, this, arguments, false))
}

async fn validate(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    is_file: bool,
) -> NativeResult {
    let function = if is_file {
        "DOMDocument::schemaValidate"
    } else {
        "DOMDocument::schemaValidateSource"
    };
    let argument = args::bytes(&arguments, 0);
    if argument.is_empty() || (is_file && argument.contains(&0)) {
        let name = if is_file { "filename" } else { "source" };
        let reason = if argument.is_empty() {
            "must not be empty"
        } else {
            "must not contain any null bytes"
        };
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #1 (${name}) {reason}"
        )));
    }
    let directory = context
        .state()
        .get_or_default::<WorkingDirectory>()
        .resolve(Path::new("."))
        .unwrap_or_else(|_| PathBuf::from("."));
    let input = if is_file {
        let name = String::from_utf8_lossy(argument);
        let Some(path) = loader::local_path(&directory, &name) else {
            context.warning(format!("{function}(): Invalid Schema file source"));
            return Ok(Value::Bool(false));
        };
        Input::File(path)
    } else {
        Input::Source(argument, directory)
    };
    let target = this_node(this.as_ref())?;
    let schema = match loader::load(input).await {
        Ok(schema) => schema,
        Err(errors) => {
            libxml::publish_schema(context, function, "", errors);
            context.warning(format!("{function}(): Invalid Schema"));
            return Ok(Value::Bool(false));
        }
    };
    let flags = args::int(&arguments, 1).unwrap_or_default();
    let (file, errors) = {
        let mut document = target.shared.borrow_mut();
        let file = document.document.settings.uri.clone().unwrap_or_default();
        let errors = schema.validate(&mut document.document.arena, flags & 1 != 0);
        (file, errors)
    };
    let valid = errors.is_empty();
    libxml::publish_schema(context, function, &file, errors);
    Ok(Value::Bool(valid))
}
