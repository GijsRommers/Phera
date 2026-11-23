//! `session_set_save_handler($handler, true)` installs an object for this request.

use std::rc::Rc;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Object, Param,
    ParamType, Value, args,
};

use crate::{
    id, settings,
    state::{can_change, state},
};

const SET_HANDLER: &[Param] = &[
    Param::new("open", ParamType::Mixed),
    Param::new("close", ParamType::Mixed).with_default(ConstantValue::Bool(true)),
    Param::new("read", ParamType::Mixed).optional(),
    Param::new("write", ParamType::Mixed).optional(),
    Param::new("destroy", ParamType::Mixed).optional(),
    Param::new("gc", ParamType::Mixed).optional(),
    Param::new("create_sid", ParamType::Mixed).with_default(ConstantValue::Null),
    Param::new("validate_sid", ParamType::Mixed).with_default(ConstantValue::Null),
    Param::new("update_timestamp", ParamType::Mixed).with_default(ConstantValue::Null),
];

pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous("session_set_save_handler", SET_HANDLER, set),
    FunctionEntry::asynchronous("session_register_shutdown", &[], register_shutdown),
];

fn set(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "session_set_save_handler";
        if arguments.len() > 2 {
            return Err(NativeError::throw(
                "Phera\\NotSupportedException",
                "The callable form of session_set_save_handler() is not supported by Phera: \
                 deprecated since PHP 8.4",
            ));
        }
        let handler = args::value(&arguments, 0).try_deref()?;
        let object = match &*handler {
            Value::Object(object) if object.class.is_instance_of("SessionHandlerInterface") => {
                object.clone()
            }
            other => {
                return Err(NativeError::type_error(format!(
                    "{function}(): Argument #1 ($open) must be of type \
                     SessionHandlerInterface, {} given",
                    given(other)
                )));
            }
        };
        drop(handler);
        let mut register = arguments.get(1).cloned().unwrap_or(Value::Bool(true));
        phera_extension_api::prepare_single_argument(
            function,
            2,
            &Param::new("close", ParamType::Bool),
            &mut register,
            context,
        )?;
        if !can_change(context, function, "save handler", true) {
            return Ok(Value::Bool(false));
        }
        let callback = match register {
            Value::Bool(false) => None,
            _ => Some("session_register_shutdown"),
        };
        context.named_shutdown("session_shutdown", callback).await?;
        if !settings::update(context, "save_handler", "user".into(), function)? {
            return Ok(Value::Bool(false));
        }
        state(context).user = Some(object);
        Ok(Value::Bool(true))
    })
}

fn register_shutdown(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    context.register_request_callback(
        phera_extension_api::RequestCallback::Shutdown,
        vec![Value::from("session_write_close")],
    )
}

pub fn handler(context: &mut dyn Context) -> Option<Rc<Object>> {
    if settings::text(context, "save_handler").as_bytes() == b"user" {
        state(context).user.clone()
    } else {
        None
    }
}

pub async fn call(
    context: &mut dyn Context,
    object: Rc<Object>,
    method: &str,
    args: Vec<Value>,
    function: &str,
) -> Result<Option<Value>, NativeError> {
    if state(context).in_handler {
        state(context).in_handler = false;
        context.warning(format!(
            "{function}(): Cannot call session save handler in a recursive manner"
        ));
        return Ok(None);
    }
    state(context).in_handler = true;
    let result = context.call_protocol_method(object, method, args).await;
    state(context).in_handler = false;
    result.map(Some)
}

pub async fn boolean(
    context: &mut dyn Context,
    object: Rc<Object>,
    method: &str,
    args: Vec<Value>,
    function: &str,
) -> Result<bool, NativeError> {
    match call(context, object, method, args, function).await? {
        Some(Value::Bool(result)) => Ok(result),
        None => Ok(false),
        Some(Value::Int(number @ (0 | -1))) => {
            context.deprecated(format!(
                "{function}(): Session callback must have a return value of type bool, int returned"
            ));
            Ok(number == 0)
        }
        Some(other) => Err(NativeError::type_error(format!(
            "Session callback must have a return value of type bool, {} returned",
            given(&other)
        ))),
    }
}

pub async fn create(
    context: &mut dyn Context,
    function: &str,
) -> Result<phera_extension_api::PhpString, NativeError> {
    if let Some(object) = handler(context)
        && !state(context).in_handler
        && context.has_method(&object, "create_sid")
    {
        return match call(context, object, "create_sid", Vec::new(), function).await? {
            Some(Value::String(id)) => Ok(id),
            Some(_) => Err(NativeError::error("Session id must be a string")),
            None => Err(NativeError::error("No session id returned by function")),
        };
    }
    id::random()
}

pub fn given(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        value => value.type_name(),
    }
}
