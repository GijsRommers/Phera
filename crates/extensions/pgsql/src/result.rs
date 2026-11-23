//! `pg_fetch_assoc($result)` reads the next row as PostgreSQL strings and nulls.

pub mod fields;
mod object;
pub use object::fetch_object;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use phera_db_drivers::drivers::postgres::NativeResult as Rows;
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::connection;

pub const ROW: usize = 0;
pub const ASSOC: usize = 1;
pub const BOTH: usize = 2;
pub const ALL: usize = 3;
pub const ALL_COLUMNS: usize = 4;
pub const VALUE: usize = 5;
pub const SEEK: usize = 6;
pub const NUM_ROWS: usize = 7;
pub const NUM_FIELDS: usize = 8;
pub const AFFECTED: usize = 9;
pub const FREE: usize = 10;
pub const STATUS: usize = 11;
pub const ERROR: usize = 12;
pub const ERROR_FIELD: usize = 13;
pub const OID: usize = 14;
pub const LENGTH: usize = 15;
pub const IS_NULL: usize = 16;

const NAMES: &[&str] = &[
    "pg_fetch_row",
    "pg_fetch_assoc",
    "pg_fetch_array",
    "pg_fetch_all",
    "pg_fetch_all_columns",
    "pg_fetch_result",
    "pg_result_seek",
    "pg_num_rows",
    "pg_num_fields",
    "pg_affected_rows",
    "pg_free_result",
    "pg_result_status",
    "pg_result_error",
    "pg_result_error_field",
    "pg_last_oid",
    "pg_field_prtlen",
    "pg_field_is_null",
];

pub struct ResultSet {
    pub rows: Rows,
    pub position: usize,
    pub connection: Option<Weak<RefCell<connection::Connection>>>,
}

type Handle = Rc<RefCell<Option<ResultSet>>>;

pub fn create(
    context: &mut dyn Context,
    rows: Rows,
    connection: Option<&connection::Handle>,
) -> NativeResult {
    let value = context.create_object("PgSql\\Result", PhpArray::new())?;
    if let Value::Object(object) = &value {
        let handle = Rc::new(RefCell::new(Some(ResultSet {
            rows,
            position: 0,
            connection: connection.map(Rc::downgrade),
        })));
        *object.native.borrow_mut() = Some(Box::new(handle));
    }
    Ok(value)
}

pub fn handle(object: &Object) -> Result<Handle, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Handle>())
        .cloned()
        .ok_or_else(closed)
}

pub fn closed() -> NativeError {
    NativeError::error("PostgreSQL result has already been closed")
}

pub fn of(arguments: &[Value]) -> Result<Handle, NativeError> {
    let object = args::object(arguments, 0).ok_or_else(closed)?;
    handle(object)
}

pub fn execute<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    execute_named::<OP>(c, a, NAMES.get(OP).copied().unwrap_or_default())
}

pub(crate) fn execute_named<const OP: usize>(
    c: &mut dyn Context,
    a: &[Value],
    call: &str,
) -> NativeResult {
    let handle = of(a)?;
    let mut result = handle.borrow_mut();
    let state = result.as_mut().ok_or_else(closed)?;
    let value = match OP {
        FREE => {
            *result = None;
            Value::Bool(true)
        }
        NUM_ROWS => Value::Int(state.rows.values.len() as i64),
        NUM_FIELDS => Value::Int(state.rows.columns.len() as i64),
        AFFECTED => Value::Int(state.rows.affected as i64),
        STATUS => match args::int(a, 1).unwrap_or(1) {
            1 => Value::Int(state.rows.status()),
            2 => Value::from(state.rows.command.clone()),
            _ => {
                return Err(NativeError::value_error(
                    "pg_result_status(): Argument #2 ($mode) must be either                  PGSQL_STATUS_LONG or PGSQL_STATUS_STRING",
                ));
            }
        },
        ERROR => state.rows.error.as_ref().map_or_else(
            || Value::from(""),
            |error| Value::from(format!("{}\n", error.detail)),
        ),
        ERROR_FIELD => match (state.rows.error.as_ref(), args::int(a, 1)) {
            (Some(error), Some(67)) => Value::from(error.state.clone()),
            (Some(error), Some(83 | 86)) => Value::from(
                error
                    .detail
                    .split_once(':')
                    .map_or("ERROR", |(severity, _)| severity),
            ),
            (Some(error), Some(77)) => Value::from(
                error
                    .detail
                    .lines()
                    .next()
                    .and_then(|line| line.split_once(":  "))
                    .map_or_else(|| error.detail.as_str(), |(_, detail)| detail),
            ),
            _ => Value::Null,
        },
        OID => Value::Bool(false),
        SEEK => {
            let row = args::int(a, 1).unwrap_or_default();
            if let Some(row) = usize::try_from(row)
                .ok()
                .filter(|i| *i < state.rows.values.len())
            {
                state.position = row;
                Value::Bool(true)
            } else {
                Value::Bool(false)
            }
        }
        VALUE | LENGTH | IS_NULL => return cell::<OP>(c, state, a, call),
        ALL | ALL_COLUMNS => {
            let mode = if OP == ALL {
                Mode::parse(args::int(a, 1).unwrap_or(1), call, 2, true)?
            } else {
                Mode::Num
            };
            let index = if OP == ALL_COLUMNS {
                Some(fields::index(state, args::int(a, 1).unwrap_or(0), call, 2)?)
            } else {
                None
            };
            let mut rows = PhpArray::new();
            for row in &state.rows.values {
                rows.push(match index {
                    Some(index) => from_cell(row.get(index).and_then(Option::as_ref)),
                    None => Value::from(state.array(row, mode)),
                })?;
            }
            Value::from(rows)
        }
        _ => {
            let mode = match OP {
                ASSOC => Mode::Assoc,
                BOTH => Mode::parse(args::int(a, 2).unwrap_or(3), call, 3, false)?,
                ROW => Mode::parse(args::int(a, 2).unwrap_or(2), call, 3, false)?,
                _ => Mode::Both,
            };
            let row = state.row(c, args::int(a, 1), call)?;
            row.and_then(|index| state.rows.values.get(index))
                .map_or(Value::Bool(false), |row| {
                    Value::from(state.array(row, mode))
                })
        }
    };
    Ok(value)
}

fn cell<const OP: usize>(
    c: &mut dyn Context,
    state: &mut ResultSet,
    a: &[Value],
    call: &str,
) -> NativeResult {
    let (row, field) = if a.len() < 3 {
        if call != "pg_result" {
            c.deprecated(format!(
                "Calling {call}() with 2 arguments is deprecated, use the 3-parameter signature \
                 with a null $row parameter instead"
            ));
        }
        (None, args::value(a, 1))
    } else {
        (args::int(a, 1), args::value(a, 2))
    };
    let index = match row {
        Some(row) => usize::try_from(row).map_err(|_| {
            NativeError::value_error(format!(
                "{call}(): Argument #2 ($row) must be greater than or equal to 0"
            ))
        })?,
        None => state.position,
    };
    let Some(values) = state.rows.values.get(index) else {
        if let Some(row) = row {
            c.warning(format!("{call}(): Unable to jump to row {row}"));
        }
        return Ok(Value::Bool(false));
    };
    if row.is_none() && OP == VALUE {
        state.position += 1;
    }
    let argument = a.len();
    let field = match &*field.deref() {
        Value::Int(field) => {
            let field = usize::try_from(*field).map_err(|_| {
                NativeError::value_error(format!(
                    "Argument #{argument} must be greater than or equal to 0"
                ))
            })?;
            if field >= state.rows.columns.len() {
                return Err(NativeError::value_error(format!(
                    "Argument #{argument} must be less than the number of fields for this result set"
                )));
            }
            field
        }
        Value::String(name) => state
            .rows
            .columns
            .iter()
            .position(|column| column.name.as_bytes() == name.as_bytes())
            .ok_or_else(|| {
                NativeError::value_error(format!(
                    "Argument #{argument} must be a field name from this result set"
                ))
            })?,
        value => {
            return Err(NativeError::type_error(format!(
                "{call}(): Argument #{argument} ($field) must be of type string|int, {} given",
                value.type_name()
            )));
        }
    };
    let cell = values.get(field).and_then(Option::as_ref);
    Ok(match OP {
        LENGTH => Value::Int(cell.map_or(0, Vec::len) as i64),
        IS_NULL => Value::Int(i64::from(cell.is_none())),
        _ => from_cell(cell),
    })
}

fn from_cell(value: Option<&Vec<u8>>) -> Value {
    value.map_or(Value::Null, |value| Value::from(value.clone()))
}

impl ResultSet {
    fn row(
        &mut self,
        c: &mut dyn Context,
        row: Option<i64>,
        call: &str,
    ) -> Result<Option<usize>, NativeError> {
        if let Some(row) = row {
            let index = usize::try_from(row)
                .ok()
                .filter(|i| *i < self.rows.values.len());
            if let Some(index) = index {
                self.position = index;
            } else {
                c.warning(format!("{call}(): Unable to jump to row {row}"));
            }
            return Ok(index);
        }
        let index = self.position;
        if self.rows.values.get(index).is_none() {
            return Ok(None);
        }
        self.position += 1;
        Ok(Some(index))
    }

    fn array(&self, row: &[Option<Vec<u8>>], mode: Mode) -> PhpArray {
        let mut result = PhpArray::new();
        for (index, (column, value)) in self.rows.columns.iter().zip(row).enumerate() {
            let value = from_cell(value.as_ref());
            let key = ArrayKey::str(column.name.as_str());
            let index = ArrayKey::Int(index as i64);
            match mode {
                Mode::Num => {
                    result.insert(index, value);
                }
                Mode::Assoc => {
                    result.insert(key, value);
                }
                Mode::Both => {
                    result.insert(index, value.clone());
                    result.insert(key, value);
                }
                Mode::AssocFirst => {
                    result.insert(key, value.clone());
                    result.insert(index, value);
                }
            }
        }
        result
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Num,
    Assoc,
    Both,
    AssocFirst,
}

impl Mode {
    fn parse(
        mode: i64,
        call: &str,
        argument: usize,
        assoc_first: bool,
    ) -> Result<Self, NativeError> {
        match mode & 3 {
            1 => Ok(Self::Assoc),
            2 => Ok(Self::Num),
            3 if assoc_first => Ok(Self::AssocFirst),
            3 => Ok(Self::Both),
            _ => Err(NativeError::value_error(format!(
                "{call}(): Argument #{argument} ($mode) must be one of PGSQL_ASSOC, PGSQL_NUM, or PGSQL_BOTH"
            ))),
        }
    }
}
