//! `pg_fetch_object($result)` hydrates columns into properties before construction.

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

use super::{Mode, closed, of};

pub fn fetch_object(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::string(&a, 2).map_or_else(|| "stdClass".into(), |s| s.to_string_lossy());
        if c.lookup_class(&name).await?.is_none() {
            return Err(NativeError::type_error(format!(
                "pg_fetch_object(): Argument #3 ($class) must be a valid class name, {name} given"
            )));
        }
        let handle = of(&a)?;
        let row = {
            let mut result = handle.borrow_mut();
            let state = result.as_mut().ok_or_else(closed)?;
            let index = state.row(c, args::int(&a, 1), "pg_fetch_object")?;
            index
                .and_then(|index| state.rows.values.get(index))
                .map(|row| state.array(row, Mode::Assoc))
        };
        let Some(row) = row else {
            return Ok(Value::Bool(false));
        };
        let value = c.create_object(&name, row)?;
        let arguments = args::array(&a, 3);
        if let Value::Object(object) = &value {
            if c.has_method(object, "__construct") {
                let arguments = arguments.map_or_else(Vec::new, |arguments| {
                    arguments.iter().map(|(_, arg)| arg.clone()).collect()
                });
                c.call_method(object.clone(), "__construct", arguments)
                    .await?;
            } else if arguments.is_some_and(|a| !a.is_empty()) {
                return Err(NativeError::value_error(
                    "pg_fetch_object(): Argument #4 ($constructor_args) must be empty                      when class has no constructor",
                ));
            }
        }
        Ok(value)
    })
}
