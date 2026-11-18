//! PHP's native value format, e.g. `serialize([true]) === 'a:1:{i:0;b:1;}'`.

mod decode;
mod encode;
mod incomplete;
pub mod wire;

use phera_extension_api::{
    ClassEntry, Extension, FunctionEntry, MethodEntry, Param, ParamType, Registry,
};

pub struct Serialize;

const SERIALIZE_PARAMS: &[Param] = &[Param::new("value", ParamType::Mixed)];
const UNSERIALIZE_PARAMS: &[Param] = &[
    Param::new("data", ParamType::String),
    Param::new("options", ParamType::Array).optional(),
];
const SERIALIZABLE: ClassEntry = ClassEntry {
    methods: &[
        MethodEntry::required("serialize", &[]),
        MethodEntry::required("unserialize", &[Param::new("data", ParamType::String)]),
    ],
    ..ClassEntry::interface("Serializable")
};

impl Extension for Serialize {
    fn name(&self) -> &'static str {
        "serialize"
    }

    fn php_module(&self) -> Option<&'static str> {
        None
    }

    fn register(&self, registry: &mut Registry) {
        registry.function(FunctionEntry::asynchronous(
            "serialize",
            SERIALIZE_PARAMS,
            |context, arguments| {
                Box::pin(async move {
                    encode::serialize(context, &arguments)
                        .await
                        .map_err(|error| context.native_call_error("serialize", &arguments, error))
                })
            },
        ));
        registry.function(FunctionEntry::asynchronous(
            "unserialize",
            UNSERIALIZE_PARAMS,
            |context, arguments| {
                Box::pin(async move {
                    decode::unserialize(context, &arguments)
                        .await
                        .map_err(|error| {
                            context.native_call_error("unserialize", &arguments, error)
                        })
                })
            },
        ));
        registry.class(SERIALIZABLE);
        registry.class(incomplete::CLASS);
    }
}
