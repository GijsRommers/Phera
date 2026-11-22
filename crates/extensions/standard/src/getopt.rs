//! `getopt('o:', ['verbose'], $rest)` scans the original command-line arguments.

use std::{collections::HashMap, rc::Rc};

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType,
    PhpArray, PhpString, Registry, RequestState, Value, args,
};

#[derive(Default)]
struct CommandLine(Vec<PhpString>);

pub(crate) fn register(registry: &mut Registry) {
    const PARAMETERS: &[Param] = &[
        Param::new("short_options", ParamType::String),
        Param::new("long_options", ParamType::Array).with_default(ConstantValue::EmptyArray),
        Param::new("rest_index", ParamType::Mixed)
            .by_ref()
            .optional(),
    ];
    registry.fork_state::<CommandLine>(phera_extension_api::fresh_state);
    registry.function(FunctionEntry::asynchronous("getopt", PARAMETERS, getopt));
}

pub fn set_arguments(state: &mut RequestState, arguments: &[String]) {
    state.get_or_default::<CommandLine>().0 = arguments
        .iter()
        .map(|argument| PhpString::from(argument.as_str()))
        .collect();
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueMode {
    None,
    Required,
    Optional,
}

impl ValueMode {
    fn parse(suffix: &[u8]) -> (Self, usize) {
        if suffix.starts_with(b"::") {
            (Self::Optional, 2)
        } else if suffix.starts_with(b":") {
            (Self::Required, 1)
        } else {
            (Self::None, 0)
        }
    }
}

fn getopt(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let mut short = HashMap::new();
        let mut remaining = args::bytes(&arguments, 0);
        while let Some((&name, suffix)) = remaining.split_first() {
            let (mode, consumed) = ValueMode::parse(suffix);
            if name != b':' {
                short.entry(name).or_insert(mode);
            }
            remaining = &suffix[consumed..];
        }
        let mut long = HashMap::new();
        if let Some(values) = args::array(&arguments, 1) {
            for value in values.values() {
                let option = context.stringify(value).await?;
                let name = option
                    .as_bytes()
                    .split(|byte| *byte == 0)
                    .next()
                    .unwrap_or_default();
                let split = name
                    .iter()
                    .position(|byte| *byte == b':')
                    .unwrap_or(name.len());
                let (mode, _) = ValueMode::parse(&name[split..]);
                long.entry(name[..split].to_vec()).or_insert(mode);
            }
        }
        let Some(command_line) = context.state().get::<CommandLine>() else {
            return Ok(Value::Bool(false));
        };
        let (result, next) = parse(&command_line.0, &short, &long)?;
        if let Some(rest) = args::reference(&arguments, 2) {
            *rest.borrow_mut() = Value::Int(next as i64);
        }
        Ok(Value::from(result))
    })
}

fn parse(
    argv: &[PhpString],
    short: &HashMap<u8, ValueMode>,
    long: &HashMap<Vec<u8>, ValueMode>,
) -> Result<(PhpArray, usize), phera_extension_api::NativeError> {
    let mut result = PhpArray::new();
    let mut index = 1;
    while let Some(argument) = argv.get(index) {
        let bytes = argument.as_bytes();
        if bytes == b"-" || !bytes.starts_with(b"-") {
            break;
        }
        index += 1;
        if let Some(option) = bytes.strip_prefix(b"--") {
            if option.is_empty() {
                break;
            }
            let (name, attached) = take_long_option(option);
            if let Some(&mode) = long.get(name)
                && attached != Some(b"".as_slice())
            {
                let value = parse_value(mode, attached, argv, &mut index);
                add_option(&mut result, name, value)?;
            }
            continue;
        }
        let mut cluster = &bytes[1..];
        while let Some((&name, tail)) = cluster.split_first() {
            cluster = tail;
            let Some(&mode) = short.get(&name) else {
                continue;
            };
            let attached = if tail.is_empty() {
                None
            } else {
                Some(tail.strip_prefix(b"=").unwrap_or(tail))
            };
            let value = parse_value(mode, attached, argv, &mut index);
            add_option(&mut result, &[name], value)?;
            if mode != ValueMode::None || tail.starts_with(b"=") {
                break;
            }
        }
    }
    Ok((result, index))
}

fn take_long_option(option: &[u8]) -> (&[u8], Option<&[u8]>) {
    match option.iter().position(|byte| *byte == b'=') {
        Some(index) => (&option[..index], Some(&option[index + 1..])),
        None => (option, None),
    }
}

fn parse_value(
    mode: ValueMode,
    attached: Option<&[u8]>,
    arguments: &[PhpString],
    index: &mut usize,
) -> Option<Value> {
    match (mode, attached) {
        (ValueMode::None, _) | (ValueMode::Optional, None) => Some(Value::Bool(false)),
        (_, Some(value)) => Some(Value::from(value)),
        (ValueMode::Required, None) => {
            let value = arguments.get(*index)?.clone();
            *index += 1;
            Some(Value::String(value))
        }
    }
}

fn add_option(
    options: &mut PhpArray,
    name: &[u8],
    value: Option<Value>,
) -> Result<(), phera_extension_api::NativeError> {
    let Some(value) = value else {
        return Ok(());
    };
    let key = ArrayKey::from_string(PhpString::from(name));
    match options.get_mut(&key) {
        Some(Value::Array(values)) => {
            Rc::make_mut(values).push(value)?;
        }
        Some(previous) => {
            *previous = Value::from(PhpArray::from_values([previous.clone(), value], 2));
        }
        None => {
            options.insert(key, value);
        }
    }
    Ok(())
}
