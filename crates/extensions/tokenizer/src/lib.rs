//! PHP's tokenizer extension. `token_name(T_STRING)` returns `"T_STRING"`.

mod errors;
mod feedback;
mod ids;
mod names;
mod parse;
mod parse_source;
mod php_token;
mod stream;
mod token;

use phera_extension_api::{
    ClassEntry, Context, Extension, FunctionEntry, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

pub struct Tokenizer;

impl Extension for Tokenizer {
    fn name(&self) -> &'static str {
        "tokenizer"
    }

    fn register(&self, registry: &mut Registry) {
        for &(name, id) in names::CONSTANTS {
            registry.constant(name, id as i64);
        }
        registry.constant("T_PAAMAYIM_NEKUDOTAYIM", ids::Id::DoubleColon as i64);
        registry.constant("TOKEN_PARSE", ids::TOKEN_PARSE);
        registry.function(FunctionEntry::sync(
            "token_get_all",
            stream::PARAMETERS,
            token_get_all,
        ));
        if registry.get_class("CompileError").is_none() {
            registry.class(ClassEntry {
                parent: Some("Error"),
                ..ClassEntry::class("CompileError")
            });
        }
        if registry.get_class("ParseError").is_none() {
            registry.class(ClassEntry {
                parent: Some("CompileError"),
                ..ClassEntry::class("ParseError")
            });
        }
        php_token::register(registry);
        registry.function(FunctionEntry::sync(
            "token_name",
            const { &[Param::new("id", ParamType::Int)] },
            token_name,
        ));
    }
}

fn token_name(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let id = args::int(arguments, 0).expect("required integer");
    Ok(Value::from(names::name(id).unwrap_or("UNKNOWN")))
}

fn token_get_all(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    stream::tokenize(
        context,
        args::bytes(args, 0),
        args::int(args, 1).unwrap_or_default(),
        |_, token| {
            let text = Value::from(token.text);
            if token.id < 256 {
                return Ok(text);
            }
            let mut entry = PhpArray::new();
            entry.push(Value::Int(token.id))?;
            entry.push(text)?;
            entry.push(Value::Int(token.line))?;
            Ok(Value::from(entry))
        },
    )
}
