//! ResourceBundle holds locale data in ordered PHP-accessible tables.

mod binary;
mod data;
mod files;
mod registration;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeIterator, NativeResult, Object, PhpArray,
    Registry, Value, args,
};

use self::data::{Node, Table};
use crate::{error, locale, locale_id, number, unsupported};

const CLASS: &str = "ResourceBundle";

#[derive(Clone)]
struct State {
    table: Table,
    inheritance: Vec<Table>,
    error: (i32, Option<String>),
}

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

fn create<'a>(
    context: &'a mut dyn Context,
    object: Option<Rc<Object>>,
    values: Vec<Value>,
    site: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        error::clear(context);
        if args::bytes(&values, 0).len() > locale::MAX_LEN {
            return locale::too_long_for(context, site, object.is_some());
        }
        if let Some(object) = &object
            && object.native_payload().map_err(|_| missing())?.is_some()
        {
            return Err(NativeError::error(
                "ResourceBundle object is already constructed",
            ));
        }
        let name = args::string(&values, 1).map(|name| String::from_utf8_lossy(name.as_bytes()));
        let name = name.as_deref().unwrap_or("ICUDATA");
        let locale = locale::effective(context, args::bytes(&values, 0));
        let fallback = args::bool(&values, 2).unwrap_or(true);
        let loaded = if data::known(name) {
            let modern = locale_id::cldr(&locale);
            (fallback || number::known_language(modern.id.language.as_str())).then(|| {
                files::Loaded {
                    direct: data::open(&modern, name),
                    inheritance: Vec::new(),
                    status: 0,
                }
            })
        } else if name.is_empty() {
            None
        } else {
            files::open(&files::path(context, name)?, &locale, fallback).await
        };
        let Some(loaded) = loaded else {
            let detail = "Cannot load libICU resource bundle";
            if object.is_some() {
                return Err(NativeError::throw(
                    "IntlException",
                    format!("{site}(): {detail}"),
                ));
            }
            return error::fail(context, site, 2, detail, Value::Null);
        };
        error::warning_code(context, loaded.status);
        let state = State {
            table: loaded.direct,
            inheritance: loaded.inheritance,
            error: (loaded.status, None),
        };
        if let Some(object) = object {
            *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
            return Ok(Value::Null);
        }
        make(context, state)
    })
}

fn make(context: &mut dyn Context, state: State) -> NativeResult {
    let object = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(object)
}

fn update<T>(object: &Object, action: impl FnOnce(&mut State) -> T) -> Result<T, NativeError> {
    let mut payload = object.native_payload_mut().map_err(|_| missing())?;
    payload
        .as_deref_mut()
        .and_then(|payload| payload.downcast_mut::<State>())
        .map(action)
        .ok_or_else(missing)
}

fn lookup(table: &Table, key: &Value) -> Option<Node> {
    let row = match &*key.deref() {
        Value::Int(index) => table.get(usize::try_from(*index).ok()?),
        Value::String(key) => table
            .iter()
            .find(|(name, _)| matches!(name, ArrayKey::String(name) if name == key)),
        _ => None,
    }?;
    Some(row.1.clone())
}

fn get(
    context: &mut dyn Context,
    object: &Object,
    index: &Value,
    fallback: bool,
    site: &str,
) -> NativeResult {
    error::clear(context);
    match &*index.deref() {
        Value::Int(index) if i32::try_from(*index).is_err() => {
            return Err(NativeError::value_error(
                "Index must be between -2147483648 and 2147483647",
            ));
        }
        Value::String(index) if index.as_bytes().is_empty() => {
            return Err(NativeError::value_error("Offset must not be empty"));
        }
        Value::Int(_) | Value::String(_) => {}
        value => {
            return Err(NativeError::type_error(format!(
                "Cannot access offset of type {} on ResourceBundle",
                value.type_name(),
            )));
        }
    }
    let value = update(object, |state| {
        state.error = (0, None);
        lookup(&state.table, index).or_else(|| {
            fallback
                .then(|| {
                    state
                        .inheritance
                        .iter()
                        .find_map(|table| lookup(table, index))
                })
                .flatten()
        })
    })?;
    if let Some(value) = value {
        return wrap(context, value);
    }
    let detail = match &*index.deref() {
        Value::String(index) => format!(
            "Cannot load resource element '{}'",
            String::from_utf8_lossy(index.as_bytes())
        ),
        Value::Int(index) => format!("Cannot load resource element {index}"),
        _ => "Cannot load resource element".to_owned(),
    };
    update(object, |state| {
        state.error = (2, Some(format!("{site}(): {detail}")))
    })?;
    error::fail(context, site, 2, &detail, Value::Null)
}

fn wrap(context: &mut dyn Context, value: Node) -> NativeResult {
    match value {
        Node::Table(table) => make(
            context,
            State {
                table,
                inheritance: Vec::new(),
                error: (0, None),
            },
        ),
        Node::Scalar(value) => Ok(value),
    }
}

fn count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    update(object, |state| state.table.len() as i64)
}

fn locales<'a>(
    context: &'a mut dyn Context,
    values: Vec<Value>,
    site: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        error::clear(context);
        let name = String::from_utf8_lossy(args::bytes(&values, 0));
        let names = if name.is_empty() || data::known(&name) {
            data::available().map(str::to_owned).collect()
        } else {
            let Some(names) = files::index(&files::path(context, &name)?).await else {
                return error::fail(
                    context,
                    site,
                    2,
                    "Cannot get locales list",
                    Value::Bool(false),
                );
            };
            names
        };
        let result: PhpArray = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| (ArrayKey::Int(index as i64), Value::from(name)))
            .collect();
        Ok(Value::from(result))
    })
}

fn last_error(object: &Object, text: bool) -> NativeResult {
    update(object, |state| {
        if !text {
            return Value::Int(i64::from(state.error.0));
        }
        let name = error::name(state.error.0);
        Value::from(
            state
                .error
                .1
                .as_ref()
                .map_or_else(|| name.to_owned(), |detail| format!("{detail}: {name}")),
        )
    })
}

fn make_cursor(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    let table = update(&object, |state| state.table.clone())?;
    Ok(Box::new(Cursor { table, index: 0 }))
}

struct Cursor {
    table: Table,
    index: usize,
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = 0;
        Ok(())
    }
    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = self.index.saturating_add(1);
        Ok(())
    }
    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        Ok(self.index < self.table.len())
    }
    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        self.table
            .get(self.index)
            .map_or(Ok(Value::Null), |(_, value)| wrap(context, value.clone()))
    }
    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(self
            .table
            .get(self.index)
            .map_or(Value::Null, |(key, _)| match key {
                ArrayKey::Int(index) => Value::Int(*index),
                ArrayKey::String(name) => Value::String(name.clone()),
            }))
    }
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed ResourceBundle")
}

fn read_only() -> NativeError {
    unsupported::item("ResourceBundle dimension mutation")
}
