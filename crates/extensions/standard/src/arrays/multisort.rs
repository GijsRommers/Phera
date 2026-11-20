//! `array_multisort($prices, SORT_ASC, $names)` sorts rows across columns.
//! The first unequal column decides, and equal rows keep their original order.

use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value,
};

use super::{IntegerKeys, append_entry, ordering::SortFlags, variable_in_use, zend_sort};
use crate::constants::{
    SORT_ASC, SORT_DESC, SORT_FLAG_CASE, SORT_LOCALE_STRING, SORT_NATURAL, SORT_NUMERIC,
    SORT_REGULAR, SORT_STRING,
};

pub(super) fn register(registry: &mut Registry) {
    const PARAMETERS: &[Param] = &[
        Param::new("array", ParamType::Untyped).prefer_ref(),
        Param::new("rest", ParamType::Untyped)
            .prefer_ref()
            .variadic(),
    ];
    registry.function_with_native_frame(FunctionEntry::asynchronous(
        "array_multisort",
        PARAMETERS,
        |context, arguments| Box::pin(sort(context, arguments)),
    ));
    registry.describe_function("array_multisort", "true", None);
}

#[derive(Clone, Copy)]
enum Direction {
    Ascending,
    Descending,
}

struct Column {
    variable: Option<Rc<RefCell<Value>>>,
    original: Rc<PhpArray>,
    entries: Vec<(ArrayKey, Value)>,
    direction: Option<Direction>,
    flags: Option<SortFlags>,
}

impl Column {
    fn new(argument: &Value, array: &Rc<PhpArray>) -> Self {
        let variable = match argument {
            Value::Reference(reference) => Some(reference.clone()),
            _ => None,
        };
        Self {
            variable,
            original: array.clone(),
            entries: array
                .iter()
                .map(|(key, value)| (key, value.clone()))
                .collect(),
            direction: None,
            flags: None,
        }
    }

    async fn compare(
        &self,
        context: &mut dyn Context,
        left: usize,
        right: usize,
    ) -> Result<Ordering, NativeError> {
        let order = self
            .flags
            .unwrap_or(SortFlags::Regular)
            .compare_unstable(context, &self.entries[left].1, &self.entries[right].1)
            .await?;
        Ok(match self.direction {
            Some(Direction::Descending) => order.reverse(),
            _ => order,
        })
    }

    fn publish(self, order: &[usize]) -> Result<(), NativeError> {
        let Some(variable) = self.variable else {
            return Ok(());
        };
        let mut target = variable.try_borrow_mut().map_err(|_| variable_in_use())?;
        // A PHP comparison can change the variable. The sort still owns the
        // original array and must not overwrite the new value.
        if !matches!(&*target, Value::Array(value) if Rc::ptr_eq(value, &self.original)) {
            return Ok(());
        }
        let mut sorted = PhpArray::with_capacity(order.len());
        for &index in order {
            let (key, value) = &self.entries[index];
            append_entry(&mut sorted, key.clone(), value, IntegerKeys::Renumber)?;
        }
        sorted.set_next_append_index_to_len();
        sorted.reset();
        *target = Value::from(sorted);
        Ok(())
    }
}

async fn sort(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let columns = columns(&arguments)?;
    let size = columns.first().map_or(0, |column| column.entries.len());
    if columns.iter().any(|column| column.entries.len() != size) {
        return Err(NativeError::value_error("Array sizes are inconsistent"));
    }
    if size == 0 {
        return Ok(Value::Bool(true));
    }
    let order = zend_sort::order_async(size, async |left, right| {
        for column in &columns {
            let order = column.compare(context, left, right).await?;
            if order != Ordering::Equal {
                return Ok(order);
            }
        }
        Ok(left.cmp(&right))
    })
    .await?;
    for column in columns {
        column.publish(&order)?;
    }
    Ok(Value::Bool(true))
}

fn columns(arguments: &[Value]) -> Result<Vec<Column>, NativeError> {
    let mut columns = Vec::new();
    for (index, argument) in arguments.iter().enumerate() {
        match &*argument.try_deref()? {
            Value::Array(array) => columns.push(Column::new(argument, array)),
            Value::Int(flag) => apply_flag(columns.last_mut(), index, *flag)?,
            _ => {
                return Err(NativeError::type_error(format!(
                    "{} must be an array or a sort flag",
                    argument_prefix(index)
                )));
            }
        }
    }
    Ok(columns)
}

fn apply_flag(column: Option<&mut Column>, index: usize, flag: i64) -> Result<(), NativeError> {
    let kind = flag & !SORT_FLAG_CASE;
    if !matches!(
        kind,
        SORT_ASC
            | SORT_DESC
            | SORT_REGULAR
            | SORT_STRING
            | SORT_NUMERIC
            | SORT_LOCALE_STRING
            | SORT_NATURAL
    ) {
        return Err(NativeError::value_error(format!(
            "{} must be a valid sort flag",
            argument_prefix(index)
        )));
    }
    let duplicate = || {
        NativeError::type_error(format!(
            "{} must be an array or a sort flag that has not already been specified",
            argument_prefix(index)
        ))
    };
    let column = column.ok_or_else(duplicate)?;
    match kind {
        SORT_ASC | SORT_DESC => {
            if column.direction.is_some() {
                return Err(duplicate());
            }
            column.direction = Some(if flag == SORT_DESC {
                Direction::Descending
            } else {
                Direction::Ascending
            });
        }
        _ => {
            if column.flags.is_some() {
                return Err(duplicate());
            }
            column.flags = Some(SortFlags::from_flags(flag));
        }
    }
    Ok(())
}

fn argument_prefix(index: usize) -> String {
    if index == 0 {
        "array_multisort(): Argument #1 ($array)".into()
    } else {
        format!("array_multisort(): Argument #{}", index + 1)
    }
}
