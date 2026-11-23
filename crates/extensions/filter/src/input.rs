//! `filter_input(INPUT_GET, 'count')` reads the original request values.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, RequestState, Value, args, copy_state,
};

use crate::{
    apply,
    constants::{
        DEFAULT, INPUT_COOKIE, INPUT_ENV, INPUT_GET, INPUT_POST, INPUT_SERVER, NULL_ON_FAILURE,
        REQUIRE_SCALAR,
    },
    functions::{apply_array, known, warn},
    options::{Call, Options},
};

// The arrays share storage with the initial superglobals. PHP writes separate
// in the usual way, so an assignment to $_GET cannot replace the input data.
#[derive(Clone, Default)]
struct Inputs {
    post: Option<Rc<PhpArray>>,
    get: Option<Rc<PhpArray>>,
    cookie: Option<Rc<PhpArray>>,
    env: Option<Rc<PhpArray>>,
    server: Option<Rc<PhpArray>>,
    server_mode: ServerMode,
}

#[derive(Clone, Copy, Default)]
enum ServerMode {
    #[default]
    Cli,
    Http,
}

impl ServerMode {
    fn exposes(self, key: &ArrayKey) -> bool {
        let Some(name) = key.as_str_bytes() else {
            return false;
        };
        match self {
            Self::Cli => matches!(name, b"SCRIPT_FILENAME" | b"SCRIPT_NAME" | b"PHP_SELF"),
            Self::Http => {
                name.starts_with(b"HTTP_")
                    || matches!(name, b"QUERY_STRING" | b"CONTENT_TYPE" | b"CONTENT_LENGTH")
            }
        }
    }
}

pub fn capture_environment(state: &mut RequestState, env: &Rc<PhpArray>) {
    state.get_or_default::<Inputs>().env = nonempty(env);
}

pub fn capture_server(state: &mut RequestState, server: &Rc<PhpArray>) {
    let inputs = state.get_or_default::<Inputs>();
    inputs.server = nonempty(server);
    inputs.server_mode = ServerMode::Cli;
}

pub fn capture_http_inputs(
    state: &mut RequestState,
    get: &Rc<PhpArray>,
    post: &Rc<PhpArray>,
    cookie: &Rc<PhpArray>,
    server: &Rc<PhpArray>,
) {
    let inputs = state.get_or_default::<Inputs>();
    inputs.get = nonempty(get);
    inputs.post = nonempty(post);
    inputs.cookie = nonempty(cookie);
    inputs.server = nonempty(server);
    inputs.server_mode = ServerMode::Http;
}

fn nonempty(value: &Rc<PhpArray>) -> Option<Rc<PhpArray>> {
    (!value.is_empty()).then(|| value.clone())
}

pub(super) fn register(registry: &mut Registry) {
    const TYPE: Param = Param::new("type", ParamType::Int);
    const NAME: Param = Param::new("var_name", ParamType::String);
    const FILTER: ConstantValue = ConstantValue::Int(DEFAULT);
    registry.fork_state::<Inputs>(copy_state::<Inputs>);
    registry.function(FunctionEntry::sync(
        "filter_has_var",
        const { &[Param::new("input_type", ParamType::Int), NAME] },
        has_var,
    ));
    registry.function(FunctionEntry::asynchronous(
        "filter_input",
        const {
            &[
                TYPE,
                NAME,
                Param::new("filter", ParamType::Int).with_default(FILTER),
                Param::new("options", ParamType::Mixed).with_default(ConstantValue::Int(0)),
            ]
        },
        input,
    ));
    registry.function(FunctionEntry::asynchronous(
        "filter_input_array",
        const {
            &[
                TYPE,
                Param::new("options", ParamType::Mixed).with_default(FILTER),
                Param::new("add_empty", ParamType::Bool).with_default(ConstantValue::Bool(true)),
            ]
        },
        input_array,
    ));
}

struct Source<'a> {
    values: Option<&'a Rc<PhpArray>>,
    server: Option<ServerMode>,
}

impl Source<'_> {
    fn get(&self, key: &ArrayKey) -> Option<&Value> {
        if self.server.is_some_and(|mode| !mode.exposes(key)) {
            return None;
        }
        self.values?.get(key)
    }

    fn array(self) -> Option<Rc<PhpArray>> {
        let values = self.values?;
        let Some(mode) = self.server else {
            return Some(values.clone());
        };
        let filtered = PhpArray::from_iter(
            values
                .iter()
                .filter(|(key, _)| mode.exposes(key))
                .map(|(key, value)| (key, value.clone())),
        );
        (!filtered.is_empty()).then(|| Rc::new(filtered))
    }
}

fn source<'a>(
    context: &'a mut dyn Context,
    arguments: &[Value],
    function: &str,
    parameter: &str,
) -> Result<Source<'a>, NativeError> {
    let fields = context.state().get::<Inputs>();
    let kind = args::int(arguments, 0).unwrap_or_default();
    let values = match kind {
        INPUT_GET => fields.and_then(|fields| fields.get.as_ref()),
        INPUT_POST => fields.and_then(|fields| fields.post.as_ref()),
        INPUT_COOKIE => fields.and_then(|fields| fields.cookie.as_ref()),
        INPUT_ENV => fields.and_then(|fields| fields.env.as_ref()),
        INPUT_SERVER => fields.and_then(|fields| fields.server.as_ref()),
        _ => {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #1 (${parameter}) must be an INPUT_* constant"
            )));
        }
    };
    let server = if kind == INPUT_SERVER {
        fields.map(|fields| fields.server_mode)
    } else {
        None
    };
    Ok(Source { values, server })
}

fn has_var(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let source = source(context, arguments, "filter_has_var", "input_type")?;
    let name = ArrayKey::from_string(args::bytes(arguments, 1).into());
    Ok(Value::Bool(source.get(&name).is_some()))
}

fn input(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let call = Call::Input;
        let name = ArrayKey::from_string(args::bytes(&arguments, 1).into());
        let value = source(context, &arguments, call.name(), "type")?
            .get(&name)
            .cloned();
        let definition = call.union(context, args::value(&arguments, 3))?;
        let id = args::int(&arguments, 2).unwrap_or(DEFAULT);
        if !known(id) {
            warn(context, call, id);
            return Ok(Value::Bool(false));
        }
        let options = Options::new(context, call, id, REQUIRE_SCALAR, &definition)?;
        let Some(value) = value else {
            return Ok(if options.flags.has(NULL_ON_FAILURE) {
                Value::Bool(false)
            } else {
                Value::Null
            });
        };
        apply::value(context, value, &options).await
    })
}

fn input_array(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let call = Call::InputArray;
        let input = source(context, &arguments, call.name(), "type")?.array();
        let definition = match arguments.get(1) {
            Some(options) => call.union(context, options)?,
            None => Value::Int(DEFAULT),
        };
        let Some(input) = input else {
            return Ok(Value::Null);
        };
        let add_empty = args::bool(&arguments, 2).unwrap_or(true);
        apply_array(context, input, definition, add_empty, call).await
    })
}
