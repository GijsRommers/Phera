//! Move XMLReader's cursor and expose parser options.
//!
//! ```php
//! while ($reader->read()) {
//!     if ($reader->nodeType === XMLReader::ELEMENT) echo $reader->name;
//! }
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::attributes::This;
use crate::cursor::Position;
use crate::state::{Reader, State, data};

pub fn read(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(step(context, this, None, true))
}

pub fn next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::string(&args, 0).map(phera_extension_api::PhpString::to_string_lossy);
        step(context, this, name.as_deref(), false).await
    })
}

async fn step(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    name: Option<&str>,
    first_child: bool,
) -> NativeResult {
    let state = loaded(this.as_ref(), "Data must be loaded before reading")?;
    let next_source = state.borrow_mut().take_source();
    if let Some(source) = next_source {
        let data = source.read(context).await?;
        let method = if first_child { "read" } else { "next" };
        state.borrow_mut().parse(context, method, &data);
    }
    let mut state = state.borrow_mut();
    let Some(tree) = &state.tree else {
        return Ok(Value::Bool(false));
    };
    let position = tree.document(|doc| {
        if first_child {
            state.position.advance(&doc.arena)
        } else {
            state.position.next(&doc.arena, name)
        }
    });
    if state.incomplete_stream
        && let Position::End { id, .. } = position
        && tree.document(|doc| !doc.arena[id].closed_element)
    {
        return Ok(Value::Bool(false));
    }
    state.position = position;
    Ok(Value::Bool(state.position.id().is_some()))
}

pub fn close(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    if let Some(object) = this {
        object.native.borrow_mut().take();
    }
    Ok(Value::Bool(true))
}

fn loaded(this: This<'_>, message: &str) -> Result<Reader, NativeError> {
    this.and_then(|object| data(object))
        .ok_or_else(|| NativeError::error(message))
}

#[derive(Clone, Copy)]
enum ParserProperty {
    LoadDtd,
    DefaultAttributes,
    Validate,
    Substitute,
}

impl ParserProperty {
    fn parse(args: &[Value], method: &str) -> Result<Self, NativeError> {
        match args::int(args, 0).unwrap_or_default() {
            1 => Ok(Self::LoadDtd),
            2 => Ok(Self::DefaultAttributes),
            3 => Ok(Self::Validate),
            4 => Ok(Self::Substitute),
            _ => Err(NativeError::value_error(format!(
                "XMLReader::{method}(): Argument #1 ($property) must be a valid parser property"
            ))),
        }
    }

    fn get(self, state: &mut State) -> &mut bool {
        match self {
            Self::LoadDtd => &mut state.load_dtd,
            Self::DefaultAttributes => &mut state.default_attributes,
            Self::Validate => &mut state.validate,
            Self::Substitute => &mut state.substitute,
        }
    }
}

pub fn get_property(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let state = loaded(this, "Cannot access parser properties before loading data")?;
    let property = ParserProperty::parse(args, "getParserProperty")?;
    Ok(Value::Bool(*property.get(&mut state.borrow_mut())))
}

pub fn set_property(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let state = loaded(this, "Cannot access parser properties before loading data")?;
    let property = ParserProperty::parse(args, "setParserProperty")?;
    let mut state = state.borrow_mut();
    if state.started() {
        return Ok(Value::Bool(false));
    }
    *property.get(&mut state) = args::bool(args, 1).unwrap_or(false);
    Ok(Value::Bool(true))
}

pub fn is_valid(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
}
