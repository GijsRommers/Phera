//! `session_start(['read_and_close' => true])` reads the data and releases its lock.

use phera_extension_api::value::{NumVal, parse_numeric};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpString, Value, args,
};

use phera_ext_serialize::Decoded;

use crate::{
    headers,
    id::c_string,
    lifecycle, settings,
    state::{active, after_output, after_start, state},
    storage, user_handler, variables,
};

pub const FUNCTION: FunctionEntry = FunctionEntry::asynchronous(
    "session_start",
    &[Param::new("options", ParamType::Array).optional()],
    start,
);

fn start(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "session_start";
        if active(context) {
            let message = after_start(
                context,
                "session_start(): Ignoring session_start() because a session is already active",
            );
            context.notice(message);
            return Ok(Value::Bool(true));
        }
        if settings::boolean(context, "use_cookies") && context.output_origin().is_some() {
            let message =
                "session_start(): Session cannot be started after headers have already been sent";
            context.warning(after_output(context, message));
            return Ok(Value::Bool(false));
        }
        let read_and_close = options(context, &arguments)?;
        let cookie = incoming_cookie(context)?;
        if state(context).id.as_ref().is_some_and(|id| {
            c_string(id.as_bytes())
                .iter()
                .any(|byte| b"\r\n\t <>'\"\\".contains(byte))
        }) {
            state(context).id = None;
        }
        let started = initialize(context, function, cookie, false).await;
        let started = match started {
            Ok(true) if active(context) => headers::cache(context, function).await?,
            Ok(_) => false,
            Err(error) => {
                if !active(context) {
                    state(context).id = None;
                } else if read_and_close {
                    storage::release(context);
                    state(context).active = false;
                } else {
                    context.register_request_finalizer(lifecycle::finalize);
                }
                return Err(error);
            }
        };
        if !started {
            lifecycle::finish(context, lifecycle::Close::Discard, function).await?;
            state(context).id = None;
            return Ok(Value::Bool(false));
        }
        if read_and_close {
            lifecycle::finish(context, lifecycle::Close::Discard, function).await?;
        } else {
            context.register_request_finalizer(lifecycle::finalize);
        }
        Ok(Value::Bool(true))
    })
}

fn options(context: &mut dyn Context, arguments: &[Value]) -> Result<bool, NativeError> {
    let mut read_and_close = false;
    let Some(options) = args::array(arguments, 0) else {
        return Ok(false);
    };
    for (key, value) in options.iter() {
        let Some(key) = key.as_str_bytes() else {
            return Err(NativeError::value_error(
                "session_start(): Argument #1 ($options) must be of type array with keys as string",
            ));
        };
        let name = String::from_utf8_lossy(key).into_owned();
        let value = value.try_deref()?;
        let text = match &*value {
            Value::InternedStr(text) => text.to_php_string(),
            Value::String(text) => text.clone(),
            Value::Int(number) => number.to_string().into(),
            Value::Bool(true) => "1".into(),
            Value::Bool(false) => "".into(),
            other => {
                return Err(NativeError::type_error(format!(
                    "session_start(): Option \"{name}\" must be of type string|int|bool, {} given",
                    user_handler::given(other)
                )));
            }
        };
        if name == "read_and_close" {
            let parsed = parse_numeric(text.as_bytes());
            read_and_close = match &*value {
                Value::Bool(boolean) => *boolean,
                Value::Int(number) => *number > 0,
                _ => match parsed.value {
                    Some(NumVal::Int(number)) if parsed.exact => number > 0,
                    _ => {
                        return Err(NativeError::type_error(format!(
                            "session_start(): Option \"{name}\" value must be of type \
                             compatible with int, \"{}\" given",
                            String::from_utf8_lossy(c_string(text.as_bytes()))
                        )));
                    }
                },
            };
        } else if !settings::update(context, &name, text, "session_start")? {
            context.warning(format!("session_start(): Setting option \"{name}\" failed"));
        }
    }
    Ok(read_and_close)
}

fn incoming_cookie(context: &mut dyn Context) -> Result<bool, NativeError> {
    if state(context).id.is_some() {
        return Ok(false);
    }
    let name = ArrayKey::String(settings::text(context, "name"));
    if settings::boolean(context, "use_cookies")
        && let Some(id) = global_id(context, "_COOKIE", &name)?
    {
        state(context).id = Some(id);
        return Ok(true);
    }
    if !settings::boolean(context, "use_only_cookies") {
        for global in ["_GET", "_POST"] {
            if global_id(context, global, &name)?.is_some() {
                return Err(NativeError::throw(
                    "Phera\\NotSupportedException",
                    "Session IDs in GET or POST are not supported by Phera: \
                     deprecated since PHP 8.4",
                ));
            }
        }
    }
    Ok(false)
}

fn global_id(
    context: &dyn Context,
    global: &str,
    name: &ArrayKey,
) -> Result<Option<PhpString>, NativeError> {
    if let Some(value) = context.request_global(global)
        && let Value::Array(array) = &*value.try_deref()?
        && let Some(value) = array.get(name)
        && let Some(id) = value.try_deref()?.to_php_string()
    {
        return Ok(Some(id));
    }
    Ok(None)
}

pub async fn initialize(
    context: &mut dyn Context,
    function: &str,
    from_cookie: bool,
    reset: bool,
) -> Result<bool, NativeError> {
    let location = context.source_location();
    state(context).active = true;
    let result = load(context, function, from_cookie, reset).await;
    match result {
        Ok(Decoded::Complete) => {
            state(context).started = location;
            Ok(true)
        }
        Ok(Decoded::HookFailed(error)) => {
            state(context).started = location;
            Err(error)
        }
        Ok(Decoded::Invalid) => {
            lifecycle::finish(context, lifecycle::Close::Discard, function).await?;
            Ok(false)
        }
        Err(error) => {
            storage::release(context);
            state(context).active = false;
            Err(error)
        }
    }
}

async fn load(
    context: &mut dyn Context,
    function: &str,
    from_cookie: bool,
    reset: bool,
) -> Result<Decoded, NativeError> {
    let path = settings::text(context, "save_path");
    let name = settings::text(context, "name");
    let reuse_files = reset && user_handler::handler(context).is_none();
    if !reuse_files && !storage::open(context, &path, name, function).await? {
        lifecycle::finish(context, lifecycle::Close::Discard, function).await?;
        let module = settings::text(context, "save_handler");
        context.warning(format!(
            "{function}(): Failed to initialize storage module: {} (path: {})",
            module.to_string_lossy(),
            path.to_string_lossy()
        ));
        return Ok(Decoded::Invalid);
    }
    let proposed = state(context).id.clone();
    let proposed = proposed.filter(|id| !c_string(id.as_bytes()).is_empty());
    let strict = settings::boolean(context, "use_strict_mode");
    let id = match proposed {
        Some(id) if !strict || storage::validate(context, &id, function).await? => Some(id),
        _ => None,
    };
    let send_cookie = (!reset && !from_cookie) || id.is_none();
    let id = match id {
        Some(id) => id,
        None => {
            state(context).id = None;
            user_handler::create(context, function).await?
        }
    };
    state(context).id = Some(id.clone());
    if send_cookie
        && settings::boolean(context, "use_cookies")
        && !headers::cookie(context, function)
    {
        return Ok(Decoded::Invalid);
    }
    variables::clear(context);
    let Some(data) = storage::read(context, &id, function).await? else {
        let module = settings::text(context, "save_handler");
        let message = format!(
            "{function}(): Failed to read session data: {} (path: {})",
            module.to_string_lossy(),
            path.to_string_lossy()
        );
        lifecycle::finish(context, lifecycle::Close::Discard, function).await?;
        context.warning(message);
        return Ok(Decoded::Invalid);
    };
    if collect_now(context) {
        storage::collect(context, function).await?;
    }
    state(context).before = settings::boolean(context, "lazy_write").then(|| data.clone());
    variables::decode(context, &data, function).await
}

fn collect_now(context: &mut dyn Context) -> bool {
    let probability = settings::integer(context, "gc_probability");
    let divisor = settings::integer(context, "gc_divisor");
    if probability <= 0 || divisor <= 0 {
        return false;
    }
    if probability >= divisor {
        return true;
    }
    let divisor = divisor as u64;
    let limit = u64::MAX - (u64::MAX % divisor);
    loop {
        let mut bytes = [0; 8];
        if getrandom::fill(&mut bytes).is_err() {
            return false;
        }
        let sample = u64::from_ne_bytes(bytes);
        if sample < limit {
            return sample % divisor < probability as u64;
        }
    }
}

pub fn require_id(context: &mut dyn Context) -> Result<PhpString, NativeError> {
    state(context)
        .id
        .clone()
        .ok_or_else(|| NativeError::error("Session ID is not initialized"))
}
