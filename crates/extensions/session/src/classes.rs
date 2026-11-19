//! Extend `SessionHandler` and call `parent::read($id)` to use the default file store.

use phera_extension_api::{
    ClassEntry, Context, MethodEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::{
    files, id,
    state::{active, state},
    user_handler,
};

const OPEN: &[Param] = &[
    Param::new("path", ParamType::String),
    Param::new("name", ParamType::String),
];
const ID: &[Param] = &[Param::new("id", ParamType::String)];
const DATA: &[Param] = &[
    Param::new("id", ParamType::String),
    Param::new("data", ParamType::String),
];
const LIFETIME: &[Param] = &[Param::new("max_lifetime", ParamType::Int)];

pub const CLASSES: &[ClassEntry] = &[
    ClassEntry {
        methods: &[
            MethodEntry::required("open", OPEN).tentative_return("bool"),
            MethodEntry::required("close", &[]).tentative_return("bool"),
            MethodEntry::required("read", ID).tentative_return("string|false"),
            MethodEntry::required("write", DATA).tentative_return("bool"),
            MethodEntry::required("destroy", ID).tentative_return("bool"),
            MethodEntry::required("gc", LIFETIME).tentative_return("int|false"),
        ],
        ..ClassEntry::interface("SessionHandlerInterface")
    },
    ClassEntry {
        methods: &[MethodEntry::required("create_sid", &[]).tentative_return("string")],
        ..ClassEntry::interface("SessionIdInterface")
    },
    ClassEntry {
        methods: &[
            MethodEntry::required("validateId", ID).tentative_return("bool"),
            MethodEntry::required("updateTimestamp", DATA).tentative_return("bool"),
        ],
        ..ClassEntry::interface("SessionUpdateTimestampHandlerInterface")
    },
    ClassEntry {
        interfaces: &["SessionHandlerInterface", "SessionIdInterface"],
        methods: METHODS,
        ..ClassEntry::class("SessionHandler")
    },
];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("open", OPEN, |context, _, arguments| {
        check(context)?;
        let path = args::string(arguments, 0).cloned().unwrap_or_default();
        Ok(Value::Bool(match files::open(context, &path) {
            Ok(()) => true,
            Err(message) => {
                context.warning(message);
                false
            }
        }))
    })
    .tentative_return("bool"),
    MethodEntry::public("close", &[], |context, _, _| {
        if !opened(context, "close")? {
            return Ok(Value::Bool(false));
        }
        state(context).files = None;
        Ok(Value::Bool(true))
    })
    .tentative_return("bool"),
    MethodEntry::public_async("read", ID, |context, _, arguments| {
        Box::pin(async move {
            if !opened(context, "read")? {
                return Ok(Value::Bool(false));
            }
            let id = args::string(&arguments, 0).cloned().unwrap_or_default();
            Ok(files::read(context, &id, "SessionHandler::read")
                .await?
                .map_or(Value::Bool(false), Value::from))
        })
    })
    .tentative_return("string|false"),
    MethodEntry::public_async("write", DATA, |context, _, arguments| {
        Box::pin(async move {
            if !opened(context, "write")? {
                return Ok(Value::Bool(false));
            }
            let id = args::string(&arguments, 0).cloned().unwrap_or_default();
            let data = args::bytes(&arguments, 1).to_vec();
            Ok(Value::Bool(
                files::write(context, &id, data, false, "SessionHandler::write").await?,
            ))
        })
    })
    .tentative_return("bool"),
    MethodEntry::public_async("destroy", ID, |context, _, arguments| {
        Box::pin(async move {
            if !opened(context, "destroy")? {
                return Ok(Value::Bool(false));
            }
            let id = args::string(&arguments, 0).cloned().unwrap_or_default();
            Ok(Value::Bool(files::destroy(context, &id).await?))
        })
    })
    .tentative_return("bool"),
    MethodEntry::public_async("gc", LIFETIME, |context, _, arguments| {
        Box::pin(async move {
            if !opened(context, "gc")? {
                return Ok(Value::Bool(false));
            }
            let lifetime = args::int(&arguments, 0).unwrap_or_default();
            Ok(files::collect(context, lifetime)
                .await?
                .map_or(Value::Bool(false), Value::Int))
        })
    })
    .tentative_return("int|false"),
    MethodEntry::public("create_sid", &[], |context, _, _| -> NativeResult {
        check(context)?;
        Ok(id::random()?.into())
    })
    .tentative_return("string"),
];

fn check(context: &mut dyn Context) -> Result<(), NativeError> {
    if !active(context) {
        return Err(NativeError::error("Session is not active"));
    }
    if user_handler::handler(context).is_none() {
        return Err(NativeError::error("Cannot call default session handler"));
    }
    Ok(())
}

fn opened(context: &mut dyn Context, method: &str) -> Result<bool, NativeError> {
    check(context)?;
    let is_open = state(context).files.is_some();
    if !is_open {
        context.warning(format!(
            "SessionHandler::{method}(): Parent session handler is not open"
        ));
    }
    Ok(is_open)
}
