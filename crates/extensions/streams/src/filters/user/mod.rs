//! `stream_filter_register('report.*', ReportFilter::class)` registers a request-local factory.

mod buckets;
mod chain;

pub(super) use chain::{attach, remove};
pub(crate) use chain::{close, lookup};

use phera_extension_api::{
    ClassEntry, ConstantValue, Context, FunctionEntry, MethodEntry, NativeError, NativeResult,
    Object, Param, ParamType, PhpArray, PropertyEntry, Registry, Value, Visibility, args,
};

const FILTER_PARAMETERS: &[Param] = &[
    Param::new("in", ParamType::Mixed),
    Param::new("out", ParamType::Mixed),
    Param::new("consumed", ParamType::Mixed).by_ref(),
    Param::new("closing", ParamType::Bool),
];

pub(super) const ERROR: i64 = 0;
pub(super) const FEED: i64 = 1;
pub(super) const PASS: i64 = 2;

#[derive(Default)]
struct Factories(Vec<(Vec<u8>, String)>);

pub(super) fn register(registry: &mut Registry) {
    buckets::register(registry);
    registry.function(FunctionEntry::sync(
        "stream_filter_register",
        const {
            &[
                Param::new("filter_name", ParamType::String),
                Param::new("class", ParamType::String),
            ]
        },
        register_class,
    ));
    registry.class(ClassEntry {
        properties: const {
            &[
                PropertyEntry::new("filtername", ConstantValue::String(""), Visibility::Public),
                PropertyEntry::new("params", ConstantValue::String(""), Visibility::Public),
                PropertyEntry::new("stream", ConstantValue::Null, Visibility::Public),
            ]
        },
        methods: const {
            &[
                MethodEntry::public("filter", FILTER_PARAMETERS, base_filter),
                MethodEntry::public("onCreate", &[], |_, _, _| Ok(Value::Bool(true))),
                MethodEntry::public("onClose", &[], |_, _, _| Ok(Value::Null)),
            ]
        },
        ..ClassEntry::class("php_user_filter")
    });
    for (name, value) in [
        ("PSFS_ERR_FATAL", ERROR),
        ("PSFS_FEED_ME", FEED),
        ("PSFS_PASS_ON", PASS),
        ("PSFS_FLAG_NORMAL", 0),
        ("PSFS_FLAG_FLUSH_INC", 1),
        ("PSFS_FLAG_FLUSH_CLOSE", 2),
    ] {
        registry.constant(name, value);
    }
}

fn base_filter(_: &mut dyn Context, _: Option<&std::rc::Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(ERROR))
}

fn register_class(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = args::bytes(arguments, 0);
    let class = args::bytes(arguments, 1);
    for (index, parameter, value) in [(1, "filter_name", name), (2, "class", class)] {
        if value.is_empty() {
            return Err(NativeError::value_error(format!(
                "stream_filter_register(): Argument #{index} (${parameter}) must be a non-empty string"
            )));
        }
    }
    let filters = context.state().get_or_default::<Factories>();
    if super::functions::BUILTINS
        .iter()
        .any(|entry| entry.as_bytes() == name)
        || filters.0.iter().any(|(entry, _)| entry == name)
    {
        return Ok(Value::Bool(false));
    }
    filters
        .0
        .push((name.to_vec(), String::from_utf8_lossy(class).into_owned()));
    Ok(Value::Bool(true))
}

pub(super) fn names(context: &mut dyn Context, names: &mut PhpArray) -> Result<(), NativeError> {
    if let Some(filters) = context.state().get::<Factories>() {
        for (name, _) in &filters.0 {
            names.push(Value::from(name.clone()))?;
        }
    }
    Ok(())
}

pub(super) fn class(context: &mut dyn Context, name: &[u8]) -> Option<String> {
    let filters = context.state().get::<Factories>()?;
    if let Some((_, class)) = filters.0.iter().find(|(entry, _)| entry == name) {
        return Some(class.clone());
    }
    name.iter().rposition(|&byte| byte == b'.')?;
    let mut end = name.len();
    while let Some(position) = name[..end].iter().rposition(|&byte| byte == b'.') {
        let prefix = &name[..=position];
        if let Some((_, class)) = filters
            .0
            .iter()
            .find(|(entry, _)| entry.strip_suffix(b"*") == Some(prefix))
        {
            return Some(class.clone());
        }
        end = position;
    }
    None
}
