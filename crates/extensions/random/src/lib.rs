//! PHP random numbers: `mt_srand(42); echo mt_rand(1,6);`.
mod classes;
mod collections;
mod engine_methods;
mod engines;
mod floats;
mod mt19937;
mod procedural;
mod randomizer;
use phera_extension_api::{Extension, FunctionEntry, Param, ParamType, Registry};
pub struct Random;
impl Extension for Random {
    fn name(&self) -> &'static str {
        "random"
    }
    fn register(&self, registry: &mut Registry) {
        registry.constant("MT_RAND_MT19937", 0i64);
        registry.constant("MT_RAND_PHP", 1i64);
        classes::register(registry);
        for function in FUNCTIONS {
            registry.function(*function);
        }
    }
}
const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "mt_srand",
        &[
            Param::new("seed", ParamType::Int).nullable().optional(),
            Param::new("mode", ParamType::Int).optional(),
        ],
        procedural::seed,
    ),
    FunctionEntry::sync(
        "srand",
        &[
            Param::new("seed", ParamType::Int).nullable().optional(),
            Param::new("mode", ParamType::Int).optional(),
        ],
        procedural::seed,
    ),
    FunctionEntry::sync(
        "mt_rand",
        &[
            Param::new("min", ParamType::Int).optional(),
            Param::new("max", ParamType::Int).optional(),
        ],
        procedural::mt_rand,
    ),
    FunctionEntry::sync(
        "rand",
        &[
            Param::new("min", ParamType::Int).optional(),
            Param::new("max", ParamType::Int).optional(),
        ],
        procedural::rand,
    ),
    FunctionEntry::sync("mt_getrandmax", &[], procedural::max),
    FunctionEntry::sync("getrandmax", &[], procedural::max),
    FunctionEntry::sync(
        "random_int",
        &[
            Param::new("min", ParamType::Int),
            Param::new("max", ParamType::Int),
        ],
        procedural::random_int,
    ),
    FunctionEntry::sync(
        "random_bytes",
        &[Param::new("length", ParamType::Int)],
        procedural::random_bytes,
    ),
    FunctionEntry::sync("lcg_value", &[], procedural::lcg),
    FunctionEntry::asynchronous(
        "uniqid",
        &[
            Param::new("prefix", ParamType::String).optional(),
            Param::new("more_entropy", ParamType::Bool).optional(),
        ],
        procedural::uniqid,
    ),
    FunctionEntry::sync(
        "shuffle",
        &[Param::new("array", ParamType::Array).by_ref()],
        collections::shuffle_array,
    ),
    FunctionEntry::sync(
        "str_shuffle",
        &[Param::new("string", ParamType::String)],
        collections::shuffle_string,
    ),
    FunctionEntry::sync(
        "array_rand",
        &[
            Param::new("array", ParamType::Array),
            Param::new("num", ParamType::Int).optional(),
        ],
        collections::array_rand,
    ),
];
