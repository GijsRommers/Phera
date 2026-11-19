//! A result is a cursor over rows returned by `mysqli_query($db, 'SELECT ...')`.

mod fields;
mod iteration;
pub use iteration::iterator;

use std::cell::RefCell;
use std::rc::Rc;

use crate::connection::properties::uint;
use phera_db_drivers::drivers::{RawRows, Rows, mysql::values};
use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, NativeResult, Object, PhpArray, Value, args,
};

pub const FREE: usize = 0;
pub const ROW: usize = 1;
pub const ASSOC: usize = 2;
pub const ARRAY: usize = 3;
pub const ALL: usize = 4;
pub const COLUMN: usize = 5;
pub const SEEK: usize = 6;
pub const FIELD_SEEK: usize = 7;
pub const FIELD_TELL: usize = 8;
pub const FIELD: usize = 9;
pub const FIELD_DIRECT: usize = 10;
pub const FIELDS: usize = 11;
pub const NUM_FIELDS: usize = 12;
pub const NUM_ROWS: usize = 13;
pub const LENGTHS: usize = 14;

type Handle = Rc<RefCell<Option<ResultSet>>>;

pub struct ResultSet {
    pub rows: Rows,
    position: usize,
    last: Option<usize>,
    field: usize,
    unbuffered: bool,
}

#[derive(Clone, Copy)]
enum Fetch {
    Assoc,
    Num,
    Both,
}

impl Fetch {
    fn parse(mode: i64, call: &str, number: usize) -> Result<Self, NativeError> {
        match mode {
            1 => Ok(Self::Assoc),
            2 => Ok(Self::Num),
            3 => Ok(Self::Both),
            _ => Err(NativeError::value_error(format!(
                "{call}(): Argument #{number} ($mode) must be one of MYSQLI_NUM, MYSQLI_ASSOC,                  or MYSQLI_BOTH"
            ))),
        }
    }
}

pub fn decode(raw: RawRows, native: bool) -> Rows {
    match raw {
        RawRows::Mysql {
            rows,
            columns,
            affected,
        } => values::convert_with_numeric_types(columns, rows, affected, native),
        _ => Rows::default(),
    }
}

pub fn create(context: &mut dyn Context, raw: RawRows, native: bool, stream: bool) -> NativeResult {
    create_rows(context, decode(raw, native), stream)
}

pub fn create_rows(context: &mut dyn Context, rows: Rows, unbuffered: bool) -> NativeResult {
    let value = context.create_object("mysqli_result", PhpArray::new())?;
    if let Value::Object(object) = &value {
        attach(object, rows, unbuffered);
    }
    Ok(value)
}

fn attach(object: &Object, rows: Rows, unbuffered: bool) {
    let set = ResultSet {
        rows,
        position: 0,
        last: None,
        field: 0,
        unbuffered,
    };
    *object.native.borrow_mut() = Some(Box::new(Rc::new(RefCell::new(Some(set)))));
}

fn handle(object: &Object) -> Result<Handle, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|value| value.downcast_ref::<Handle>())
        .cloned()
        .ok_or_else(|| NativeError::error("mysqli_result object is not fully initialized"))
}

fn closed() -> NativeError {
    NativeError::error("mysqli_result object is already closed")
}

pub fn construct(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let object = this.ok_or_else(closed)?;
    let connection = crate::connection::handle(args::object(arguments, 0))?;
    let mut connection = connection.borrow_mut();
    connection.lease()?;
    let raw = connection
        .pending
        .take()
        .ok_or_else(|| NativeError::error("mysqli_result object is not fully initialized"))?;
    let unbuffered = args::int(arguments, 1) == Some(1);
    attach(
        object,
        decode(raw.rows, connection.native_numbers),
        unbuffered,
    );
    Ok(Value::Null)
}

impl ResultSet {
    fn take(&mut self) -> Option<&[Value]> {
        let index = self.position;
        let row = self.rows.values.get(index);
        self.last = row.map(|_| index);
        self.position = index.saturating_add(1);
        row.map(Vec::as_slice)
    }

    fn array(&mut self, mode: Fetch) -> Result<Option<PhpArray>, NativeError> {
        let row = self.take();
        if row.is_none() {
            return Ok(None);
        }
        let Some(values) = self.last.and_then(|index| self.rows.values.get(index)) else {
            return Ok(None);
        };
        let mut row = PhpArray::new();
        for (index, (column, value)) in self.rows.columns.iter().zip(values).enumerate() {
            if matches!(mode, Fetch::Num | Fetch::Both) {
                row.insert(ArrayKey::Int(index as i64), value.clone());
            }
            if matches!(mode, Fetch::Assoc | Fetch::Both) {
                row.insert(ArrayKey::str(column.name.as_str()), value.clone());
            }
        }
        Ok(Some(row))
    }

    fn lengths(&self, context: &mut dyn Context) -> NativeResult {
        let Some(row) = self.last.and_then(|index| self.rows.values.get(index)) else {
            return Ok(Value::Null);
        };
        let mut lengths = PhpArray::new();
        for value in row {
            let length = to_php_string(value, context.diagnostics())?
                .as_bytes()
                .len();
            lengths.push(Value::Int(length as i64))?;
        }
        Ok(Value::from(lengths))
    }

    fn num_rows(&self) -> Value {
        uint(
            if self.unbuffered && self.position <= self.rows.values.len() {
                0
            } else {
                self.rows.values.len() as u64
            },
        )
    }
}

pub fn execute<const OP: usize>(
    context: &mut dyn Context,
    object: &Object,
    arguments: &[Value],
    call: &str,
) -> NativeResult {
    let handle = handle(object)?;
    let mut state = handle.borrow_mut();
    let results = state.as_mut().ok_or_else(closed)?;
    let raw_index = args::int(arguments, 0).unwrap_or_default();
    let index = usize::try_from(raw_index).ok();
    let arg = if call.starts_with("mysqli_result::") {
        1
    } else {
        2
    };
    match OP {
        FREE => {
            *state = None;
            Ok(Value::Null)
        }
        FIELD_SEEK | FIELD_TELL | FIELD | FIELD_DIRECT | FIELDS => {
            fields::execute::<OP>(context, results, raw_index, call, arg)
        }
        NUM_FIELDS => Ok(Value::Int(results.rows.columns.len() as i64)),
        NUM_ROWS => Ok(results.num_rows()),
        LENGTHS => Ok(match results.lengths(context)? {
            Value::Null => Value::Bool(false),
            value => value,
        }),
        SEEK => {
            if raw_index < 0 {
                return Err(NativeError::value_error(format!(
                    "{call}(): Argument #{arg} ($offset) must be greater than or equal to 0"
                )));
            }
            if results.unbuffered {
                return Err(NativeError::error(format!(
                    "{call}() cannot be used in MYSQLI_USE_RESULT mode"
                )));
            }
            let Some(index) = index.filter(|index| *index < results.rows.values.len()) else {
                return Ok(Value::Bool(false));
            };
            results.position = index;
            Ok(Value::Bool(true))
        }
        COLUMN => {
            let Some(index) = index.filter(|index| *index < results.rows.columns.len()) else {
                return Err(NativeError::value_error(format!(
                    "{call}(): Argument #{arg} ($column) must be a valid column index"
                )));
            };
            Ok(results
                .take()
                .and_then(|row| row.get(index))
                .cloned()
                .unwrap_or(Value::Bool(false)))
        }
        _ => {
            let mode = match OP {
                ROW => Fetch::Num,
                ASSOC => Fetch::Assoc,
                _ => Fetch::parse(
                    args::int(arguments, 0).unwrap_or(if OP == ALL { 2 } else { 3 }),
                    call,
                    arg,
                )?,
            };
            if OP == ALL {
                let mut all = PhpArray::new();
                while let Some(row) = results.array(mode)? {
                    all.push(Value::from(row))?;
                }
                return Ok(Value::from(all));
            }
            Ok(results.array(mode)?.map_or(Value::Null, Value::from))
        }
    }
}

pub async fn fetch_object(
    context: &mut dyn Context,
    object: &Object,
    arguments: &[Value],
    call: &str,
) -> NativeResult {
    let name =
        args::string(arguments, 0).map_or_else(|| "stdClass".into(), |s| s.to_string_lossy());
    let params = args::array(arguments, 1);
    if context.lookup_class(&name).await?.is_none() {
        let argument = if call.starts_with("mysqli_result::") {
            1
        } else {
            2
        };
        return Err(NativeError::type_error(format!(
            "{call}(): Argument #{argument} ($class) must be a valid class name, {name} given"
        )));
    }
    let handle = handle(object)?;
    let row = {
        let mut state = handle.borrow_mut();
        state.as_mut().ok_or_else(closed)?.array(Fetch::Assoc)?
    };
    let Some(row) = row else {
        return Ok(Value::Null);
    };
    let result = context.create_object(&name, row)?;
    if let Value::Object(object) = &result {
        let constructor = context.has_method(object, "__construct");
        if constructor {
            let arguments =
                params.map_or_else(Vec::new, |p| p.iter().map(|(_, v)| v.clone()).collect());
            context
                .call_method(object.clone(), "__construct", arguments)
                .await?;
        } else if params.is_some_and(|values| !values.is_empty()) {
            return Err(NativeError::value_error(format!(
                "{call}(): Argument ($constructor_args) must be empty when class has no constructor"
            )));
        }
    }
    Ok(result)
}

pub static ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if !matches!(
        name,
        "current_field" | "field_count" | "lengths" | "num_rows" | "type"
    ) {
        return Ok(None);
    }
    let handle = handle(object)?;
    let state = handle.borrow();
    let results = state.as_ref().ok_or_else(closed)?;
    Ok(Some(match name {
        "current_field" => Value::Int(results.field as i64),
        "field_count" => Value::Int(results.rows.columns.len() as i64),
        "lengths" => results.lengths(context)?,
        "num_rows" => results.num_rows(),
        _ => Value::Int(i64::from(results.unbuffered)),
    }))
}

fn mutate(
    _: &mut dyn Context,
    _: &Object,
    name: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if matches!(
        name,
        "current_field" | "field_count" | "lengths" | "num_rows" | "type"
    ) {
        return Err(NativeError::error(format!(
            "Cannot write read-only property mysqli_result::${name}"
        )));
    }
    Ok(false)
}
