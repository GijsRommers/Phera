//! Keep each XMLParser and its callbacks in one PHP object.
//!
//! ```php
//! $parser = xml_parser_create();
//! xml_set_element_handler($parser, fn ($parser, $name) => print($name), null);
//! xml_parse($parser, '<item/>', true);
//! ```

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args, value::to_bool,
};

use crate::events::{Handler, Parser, Position};
use crate::options::{Codec, OptionName};
use crate::structure::Structure;

pub const CLASS: &str = "XMLParser";

pub struct State {
    pub parser: RefCell<Parser>,
    pub handlers: RefCell<HashMap<Handler, Value>>,
    pub parsing: Cell<bool>,
}

fn state(object: &Object) -> Result<Rc<State>, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Rc<State>>())
        .cloned()
        .ok_or_else(|| NativeError::error("Invalid XMLParser object"))
}

pub fn receiver(arguments: &[Value]) -> Result<(&Rc<Object>, Rc<State>), NativeError> {
    let object =
        args::object(arguments, 0).ok_or_else(|| NativeError::error("Invalid XMLParser object"))?;
    Ok((object, state(object)?))
}

pub fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct XMLParser, use xml_parser_create() or xml_parser_create_ns() instead",
    ))
}

pub fn create(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    create_parser(context, arguments, false)
}

pub fn create_ns(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    create_parser(context, arguments, true)
}

fn create_parser(context: &mut dyn Context, arguments: &[Value], namespaces: bool) -> NativeResult {
    let function = if namespaces {
        "xml_parser_create_ns"
    } else {
        "xml_parser_create"
    };
    let encoding = args::string(arguments, 0).map(phera_extension_api::PhpString::to_string_lossy);
    let codec = encoding
        .filter(|encoding| !encoding.is_empty())
        .map(|encoding| {
            Codec::from_label(&encoding).ok_or_else(|| {
                NativeError::value_error(format!(
                    "{function}(): Argument #1 ($encoding) is not a supported source encoding"
                ))
            })
        })
        .transpose()?;
    let separator = namespaces.then(|| {
        args::string(arguments, 1).map_or(
            ":".to_owned(),
            phera_extension_api::PhpString::to_string_lossy,
        )
    });
    let value = context.create_object(CLASS, PhpArray::new())?;
    let Value::Object(ref object) = value else {
        return Err(NativeError::error("Could not create XMLParser"));
    };
    *object.native.borrow_mut() = Some(Box::new(Rc::new(State {
        parser: RefCell::new(Parser::new(codec, separator)),
        handlers: RefCell::default(),
        parsing: Cell::new(false),
    })));
    Ok(value)
}

pub fn parse(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (object, state) = receiver(&arguments)?;
        let input = args::bytes(&arguments, 1);
        let final_chunk = args::bool(&arguments, 2).unwrap_or(false);
        execute(context, object, &state, input, final_chunk, None).await
    })
}

pub fn parse_into_struct(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (object, state) = receiver(&arguments)?;
        if state.parsing.get() {
            context
                .warning("xml_parse_into_struct(): Parser must not be called recursively".into());
            return Ok(Value::Bool(false));
        }
        let mut structure = Structure::default();
        let input = args::bytes(&arguments, 1);
        let index_output = arguments.get(3).and_then(prepare_output);
        let values_output = arguments.get(2).and_then(prepare_output);
        let status = execute(context, object, &state, input, true, Some(&mut structure)).await;
        let (values, index) = structure.finish(state.parser.borrow().options.skip_white);
        finish_output(arguments.get(2), values_output.as_ref(), values);
        finish_output(arguments.get(3), index_output.as_ref(), index);
        status
    })
}

fn prepare_output(argument: &Value) -> Option<Rc<PhpArray>> {
    let Value::Reference(output) = argument else {
        return None;
    };
    let array = Rc::new(PhpArray::new());
    *output.borrow_mut() = Value::Array(array.clone());
    Some(array)
}

fn finish_output(argument: Option<&Value>, initial: Option<&Rc<PhpArray>>, value: Value) {
    if let (Some(Value::Reference(output)), Some(initial)) = (argument, initial) {
        let mut target = output.borrow_mut();
        if matches!(&*target, Value::Array(current) if Rc::ptr_eq(initial, current)) {
            *target = value;
        }
    }
}

struct Parsing<'a>(&'a Cell<bool>);

impl Drop for Parsing<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

async fn execute(
    context: &mut dyn Context,
    object: &Rc<Object>,
    state: &State,
    bytes: &[u8],
    final_chunk: bool,
    mut structure: Option<&mut Structure>,
) -> NativeResult {
    if state.parsing.replace(true) {
        return Err(NativeError::error("Parser must not be called recursively"));
    }
    let _parsing = Parsing(&state.parsing);
    if !state.parser.borrow_mut().append(bytes) {
        return Ok(Value::Int(0));
    }
    loop {
        let has_default = state.handlers.borrow().contains_key(&Handler::Default);
        let events = state.parser.borrow_mut().next(final_chunk, has_default);
        let Some(events) = events else {
            break;
        };
        let Ok(events) = events else {
            state.parser.borrow_mut().compact();
            return Ok(Value::Int(0));
        };
        for event in events {
            if let Some(structure) = &mut structure {
                if structure.observe(&event) {
                    context.warning(
                        "xml_parse_into_struct(): Maximum depth exceeded - Results truncated"
                            .into(),
                    );
                }
            }
            let callback = {
                let handlers = state.handlers.borrow();
                handlers
                    .get(&event.handler)
                    .map(|handler| (handler.clone(), event.data))
                    .or_else(|| {
                        let data = event.fallback?;
                        Some((handlers.get(&Handler::Default)?.clone(), vec![data]))
                    })
            };
            if let Some((handler, data)) = callback {
                state.parser.borrow_mut().position = event.position;
                let mut arguments = Vec::with_capacity(data.len() + 1);
                arguments.push(Value::Object(object.clone()));
                arguments.extend(data);
                let result = context.call(&handler, arguments).await?;
                if event.handler == Handler::External && !to_bool(&result) {
                    state.parser.borrow_mut().error = 21;
                    return Ok(Value::Int(0));
                }
            }
        }
    }
    state.parser.borrow_mut().compact();
    Ok(Value::Int(i64::from(state.parser.borrow().error == 0)))
}

pub fn error_code(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (_, state) = receiver(arguments)?;
    Ok(Value::Int(state.parser.borrow().error))
}

fn position(arguments: &[Value]) -> Result<Position, NativeError> {
    let (_, state) = receiver(arguments)?;
    let position = state.parser.borrow().position;
    Ok(position)
}

pub fn line(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(position(arguments)?.line as i64))
}

pub fn column(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(position(arguments)?.column as i64))
}

pub fn byte(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(position(arguments)?.offset as i64))
}

pub fn free(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    context.deprecated(
        "Function xml_parser_free() is deprecated since 8.5, as it has no effect since PHP 8.0"
            .into(),
    );
    let (_, state) = receiver(arguments)?;
    if state.parsing.get() {
        context.warning("xml_parser_free(): Parser cannot be freed while it is parsing".into());
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(true))
}

pub fn set_option(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (_, state) = receiver(arguments)?;
    let option = OptionName::read(
        args::int(arguments, 1).unwrap_or_default(),
        "xml_parser_set_option",
    )?;
    if matches!(option, OptionName::Huge) && state.parsing.get() {
        return Err(NativeError::error(
            "Cannot change option XML_OPTION_PARSE_HUGE while parsing",
        ));
    }
    let value = args::value(arguments, 2);
    Ok(Value::Bool(option.set(
        &mut state.parser.borrow_mut().options,
        context,
        value,
    )?))
}

pub fn get_option(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (_, state) = receiver(arguments)?;
    let option = OptionName::read(
        args::int(arguments, 1).unwrap_or_default(),
        "xml_parser_get_option",
    )?;
    Ok(option.get(&state.parser.borrow().options))
}
