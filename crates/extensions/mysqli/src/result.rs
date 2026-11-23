//! A result is a cursor over rows returned by `mysqli_query($db, 'SELECT ...')`.

mod fields;
mod iteration;
mod object;
mod properties;
pub(crate) mod stream;

pub use iteration::{get_iterator, iterator};
pub use object::fetch_object;
pub use properties::ACCESS;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use crate::connection::properties::uint;
use phera_db_drivers::drivers::{RawRows, Rows, mysql::values};
use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
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
    keys: OnceCell<Vec<ArrayKey>>,
    position: usize,
    last: Option<usize>,
    field: usize,
    unbuffered: bool,
    stream: Option<stream::Stream>,
    stream_count: Option<usize>,
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
                "{call}(): Argument #{number} ($mode) must be one of MYSQLI_NUM, MYSQLI_ASSOC, \
                 or MYSQLI_BOTH"
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
        } => values::convert_mysqlnd(columns, rows, affected, native),
        _ => Rows::default(),
    }
}

pub fn create(context: &mut dyn Context, raw: RawRows, native: bool, stream: bool) -> NativeResult {
    create_rows(context, decode(raw, native), stream)
}

pub fn create_rows(context: &mut dyn Context, rows: Rows, unbuffered: bool) -> NativeResult {
    let value = context.create_object("mysqli_result", PhpArray::new())?;
    if let Value::Object(object) = &value {
        attach(object, rows, unbuffered, None);
    }
    Ok(value)
}

fn attach(object: &Object, rows: Rows, unbuffered: bool, stream: Option<stream::Stream>) {
    let set = ResultSet {
        rows,
        keys: OnceCell::new(),
        position: 0,
        last: None,
        field: 0,
        unbuffered,
        stream_count: stream.as_ref().map(|_| 0),
        stream,
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

/// The fork hook of `mysqli_result`: a stored result is rows in memory, and
/// each fork reads its own copy from the same position. A result still
/// streaming from the server (`MYSQLI_USE_RESULT`) belongs to its session.
///
/// ```php
/// $options = $db->query('SELECT * FROM options');   // stored
/// Phera\snapshot();
/// $options->fetch_assoc();   // every request: the first row
/// ```
pub fn fork_result(
    _object: &Object,
    state: &dyn std::any::Any,
    _fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    let handle = state
        .downcast_ref::<Handle>()
        .ok_or("its built-in state is not initialized")?;
    let copy = match handle.borrow().as_ref() {
        None => None,
        Some(set) if set.stream.is_some() => {
            return Err("it is still being read from the server (MYSQLI_USE_RESULT); read it to the end or free it before Phera\\snapshot()".into());
        }
        Some(set) => Some(ResultSet {
            rows: set.rows.clone(),
            keys: OnceCell::new(),
            position: set.position,
            last: set.last,
            field: set.field,
            unbuffered: set.unbuffered,
            stream: None,
            stream_count: set.stream_count,
        }),
    };
    Ok(Box::new(Rc::new(RefCell::new(copy))) as Box<dyn std::any::Any>)
}

fn closed() -> NativeError {
    NativeError::error("mysqli_result object is already closed")
}

pub fn construct(
    _: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = this.ok_or_else(closed)?;
        let connection = crate::connection::handle(args::object(&arguments, 0))?;
        let (pending, native) = {
            let mut connection = connection.borrow_mut();
            connection.ensure_open()?;
            let pending = connection.pending.take().ok_or_else(|| {
                NativeError::error("mysqli_result object is not fully initialized")
            })?;
            (pending, connection.native_numbers)
        };
        let unbuffered = args::int(&arguments, 1) == Some(1);
        attach_pending(&object, pending, native, unbuffered).await?;
        Ok(Value::Null)
    })
}

pub async fn create_pending(
    context: &mut dyn Context,
    pending: crate::connection::query::Pending,
    native: bool,
    unbuffered: bool,
) -> NativeResult {
    let value = context.create_object("mysqli_result", PhpArray::new())?;
    if let Value::Object(object) = &value {
        attach_pending(object, pending, native, unbuffered).await?;
    }
    Ok(value)
}

async fn attach_pending(
    object: &Object,
    mut pending: crate::connection::query::Pending,
    native: bool,
    unbuffered: bool,
) -> Result<(), NativeError> {
    let mut rows = decode(pending.rows, native);
    if !unbuffered && let Some(stream) = pending.stream.take() {
        rows.values = stream
            .read(true)
            .await
            .map_err(|e| NativeError::error(e.detail))?
            .0
            .values;
        stream.complete(rows.values.len() as u64);
    }
    attach(object, rows, unbuffered, pending.stream);
    Ok(())
}

impl ResultSet {
    fn next_index(&mut self) -> Option<usize> {
        let index = self.position;
        self.last = (index < self.rows.values.len()).then_some(index);
        self.position = index.saturating_add(1);
        self.last
    }

    fn take(&mut self) -> Option<&[Value]> {
        let index = self.next_index()?;
        self.rows.values.get(index)
    }

    fn array(&mut self, mode: Fetch) -> Result<Option<PhpArray>, NativeError> {
        let Some(values) = self
            .next_index()
            .and_then(|index| self.rows.values.get(index))
        else {
            return Ok(None);
        };
        let mut row = PhpArray::with_capacity(values.len());
        if matches!(mode, Fetch::Num) {
            for value in values {
                row.push(value.clone())?;
            }
        } else {
            let keys = self.keys.get_or_init(|| {
                self.rows
                    .columns
                    .iter()
                    .map(|column| ArrayKey::from_string(column.name.as_str().into()))
                    .collect()
            });
            for (index, (value, key)) in values.iter().zip(keys).enumerate() {
                if matches!(mode, Fetch::Both) {
                    row.insert(ArrayKey::Int(index as i64), value.clone());
                }
                row.insert(key.clone(), value.clone());
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
        if let Some(count) = self.stream_count {
            return uint(if self.stream.is_some() {
                0
            } else {
                count as u64
            });
        }
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
                let remaining = results.rows.values.len().saturating_sub(results.position);
                let mut all = PhpArray::with_capacity(remaining);
                while let Some(row) = results.array(mode)? {
                    all.push(Value::from(row))?;
                }
                return Ok(Value::from(all));
            }
            Ok(results.array(mode)?.map_or(Value::Null, Value::from))
        }
    }
}
