//! ResourceBundle holds locale data in ordered PHP-accessible tables.

mod data;
mod registration;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeIterator, NativeResult, Object, PhpArray, Registry,
    Value, args,
};

use self::data::{Node, Table};
use crate::{error, locale, locale_id, number, unsupported};

const CLASS: &str = "ResourceBundle";

#[derive(Clone)]
struct State {
    table: Table,
    error: (i32, Option<String>),
}

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

fn create(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    site: &str,
) -> NativeResult {
    error::clear(context);
    let name = args::string(values, 1).map(|name| String::from_utf8_lossy(name.as_bytes()));
    let name = name.as_deref().unwrap_or("ICUDATA");
    let locale = locale::effective(context, args::bytes(values, 0));
    let modern = locale_id::cldr(&locale);
    let fallback = args::bool(values, 2).unwrap_or(true);
    if !data::known(name) || (!fallback && !number::known_language(modern.id.language.as_str())) {
        let detail = "Cannot load libICU resource bundle";
        if object.is_some() {
            return Err(NativeError::throw(
                "IntlException",
                format!("{site}(): {detail}"),
            ));
        }
        return error::fail(context, site, 2, detail, Value::Null);
    }
    let table = data::open(&modern, name);
    if let Some(object) = object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(State {
            table,
            error: (0, None),
        }));
        return Ok(Value::Null);
    }
    make(context, table)
}

fn make(context: &mut dyn Context, table: Table) -> NativeResult {
    let object = context.create_object(CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(State {
            table,
            error: (0, None),
        }));
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
            .find(|(name, _)| name.as_bytes() == key.as_bytes()),
        _ => None,
    }?;
    Some(row.1.clone())
}

fn get(context: &mut dyn Context, object: &Object, index: &Value, site: &str) -> NativeResult {
    error::clear(context);
    let value = update(object, |state| {
        state.error = (0, None);
        lookup(&state.table, index)
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
        Node::Table(table) => make(context, table),
        Node::Scalar(value) => Ok(value),
    }
}

fn count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    update(object, |state| state.table.len() as i64)
}

fn locales(context: &mut dyn Context, values: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    let name = String::from_utf8_lossy(args::bytes(values, 0));
    if !name.is_empty() && !data::known(&name) {
        return error::fail(
            context,
            site,
            2,
            "Cannot get locales list",
            Value::Bool(false),
        );
    }
    let result: PhpArray = data::available()
        .enumerate()
        .map(|(index, name)| (ArrayKey::Int(index as i64), Value::from(name)))
        .collect();
    Ok(Value::from(result))
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
            .map_or(Value::Null, |(key, _)| Value::from(key.as_str())))
    }
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed ResourceBundle")
}

fn read_only() -> NativeError {
    unsupported::item("ResourceBundle dimension mutation")
}
