//! `$result->fetch_object()` hydrates one row and calls the chosen constructor.

use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use super::{Fetch, closed, handle};

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
    if !super::stream::load(context, object, false, call).await? {
        return Ok(Value::Bool(false));
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
