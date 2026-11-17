//! PHP's tokenizer extension. `token_name(T_STRING)` returns `"T_STRING"`.

mod ids;

use phera_extension_api::{
    Context, Extension, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

pub struct Tokenizer;

impl Extension for Tokenizer {
    fn name(&self) -> &'static str {
        "tokenizer"
    }

    fn register(&self, registry: &mut Registry) {
        for &(name, id) in ids::CONSTANTS {
            registry.constant(name, id as i64);
        }
        registry.constant("T_PAAMAYIM_NEKUDOTAYIM", ids::Id::DoubleColon as i64);
        registry.constant("TOKEN_PARSE", ids::TOKEN_PARSE);
        registry.function(FunctionEntry::sync(
            "token_name",
            const { &[Param::new("id", ParamType::Int)] },
            token_name,
        ));
    }
}

fn token_name(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let id = args::int(arguments, 0).expect("required integer");
    Ok(Value::from(ids::name(id).unwrap_or("UNKNOWN")))
}
