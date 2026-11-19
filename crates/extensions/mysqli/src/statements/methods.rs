//! Fetching prepared results either copies rows into references or makes a mysqli_result.

use phera_db_drivers::drivers::{RawRows, Rows};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

use super::{State, Status, binding, handle};
use crate::result;

pub const BIND_PARAM: usize = 0;
pub const BIND_RESULT: usize = 1;
pub const FETCH: usize = 2;
pub const STORE: usize = 3;
pub const RESULT: usize = 4;
pub const META: usize = 5;
pub const LONG_DATA: usize = 6;
pub const RESET: usize = 7;
pub const CLOSE: usize = 8;
pub const FREE: usize = 9;
pub const SEEK: usize = 10;
pub const MORE: usize = 11;
pub const NEXT: usize = 12;

pub fn execute<const OP: usize>(
    context: &mut dyn Context,
    object: &Rc<Object>,
    arguments: &[Value],
    call: &str,
) -> NativeResult {
    let handle = handle(object)?;
    let mut state = handle.borrow_mut();
    if OP == CLOSE {
        state.status = Status::Closed;
        state.rows = None;
        state.parameters.clear();
        state.output.clear();
        state.long_data.clear();
        return Ok(Value::Bool(true));
    }
    let statement = state.statement()?;
    let arg = if call.contains("::") { 1 } else { 2 };
    match OP {
        BIND_PARAM => binding::bind(&mut state, arguments, call, arg),
        BIND_RESULT => binding::bind_result(&mut state, arguments),
        FETCH => fetch(&mut state),
        STORE => {
            state.stored = state
                .rows
                .as_ref()
                .is_some_and(|rows| !rows.columns.is_empty());
            Ok(Value::Bool(state.stored))
        }
        RESULT => {
            if let Some(rows) = state.rows.take().filter(|rows| !rows.columns.is_empty()) {
                state.reply.affected = rows.values.len() as u64;
                return result::create_rows(context, rows, false);
            }
            Ok(Value::Bool(false))
        }
        META => {
            let columns = statement.columns().to_vec();
            if columns.is_empty() {
                return Ok(Value::Bool(false));
            }
            result::create(
                context,
                RawRows::Mysql {
                    columns,
                    rows: Vec::new(),
                    affected: 0,
                },
                true,
                false,
            )
        }
        LONG_DATA => {
            let index = args::int(arguments, 0).unwrap_or_default();
            let Some(index) = usize::try_from(index)
                .ok()
                .filter(|index| *index < usize::from(statement.num_params()))
            else {
                return Err(NativeError::value_error(format!(
                    "{call}(): Argument #{arg} ($param_num) must be less than the number of parameters"
                )));
            };
            state
                .long_data
                .entry(index)
                .or_default()
                .extend_from_slice(args::bytes(arguments, 1));
            Ok(Value::Bool(true))
        }
        RESET => {
            clear(&mut state);
            state.long_data.clear();
            state.error = None;
            Ok(Value::Bool(true))
        }
        FREE => {
            clear(&mut state);
            Ok(Value::Null)
        }
        SEEK => {
            if let Some(index) =
                args::int(arguments, 0).and_then(|index| usize::try_from(index).ok())
            {
                state.row = index;
            }
            Ok(Value::Null)
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn clear(state: &mut State) {
    state.rows = None;
    state.row = 0;
    state.stored = false;
}

fn fetch(state: &mut State) -> NativeResult {
    let Some(Rows { values, .. }) = &state.rows else {
        return Ok(Value::Null);
    };
    let Some(row) = values.get(state.row) else {
        return Ok(Value::Null);
    };
    for (reference, value) in state.output.iter().zip(row) {
        *reference
            .try_borrow_mut()
            .map_err(|_| NativeError::error("Cannot update bound result"))? = value.clone();
    }
    state.row = state.row.saturating_add(1);
    Ok(Value::Bool(true))
}
