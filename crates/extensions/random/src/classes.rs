//! Native namespaced registration: `new Random\Engine\Mt19937(42)`.
use crate::{engine_methods, randomizer};
use phera_extension_api::{
    ClassEntry, ClassKind, ConstantValue, MethodEntry, Param, ParamType, PropertyEntry, Registry,
    Visibility,
};
pub fn register(registry: &mut Registry) {
    for entry in CLASSES {
        registry.class(*entry);
    }
}
const CLASSES: &[ClassEntry] = &[
    ClassEntry {
        methods: &[MethodEntry::required("generate", &[])],
        ..ClassEntry::interface("Random\\Engine")
    },
    ClassEntry {
        interfaces: &["Random\\Engine"],
        ..ClassEntry::interface("Random\\CryptoSafeEngine")
    },
    ClassEntry {
        parent: Some("Error"),
        ..ClassEntry::class("Random\\RandomError")
    },
    ClassEntry {
        parent: Some("Random\\RandomError"),
        ..ClassEntry::class("Random\\BrokenRandomEngineError")
    },
    ClassEntry {
        parent: Some("Exception"),
        ..ClassEntry::class("Random\\RandomException")
    },
    ClassEntry {
        kind: ClassKind::FinalClass,
        interfaces: &["Random\\Engine"],
        methods: &[
            MethodEntry::public(
                "__construct",
                &[
                    Param::new("seed", ParamType::Int).nullable().optional(),
                    Param::new("mode", ParamType::Int).optional(),
                ],
                engine_methods::mt,
            ),
            MethodEntry::public("generate", &[], engine_methods::generate),
        ],
        ..ClassEntry::class("Random\\Engine\\Mt19937")
    },
    ClassEntry {
        kind: ClassKind::FinalClass,
        interfaces: &["Random\\Engine"],
        methods: &[
            MethodEntry::public(
                "__construct",
                &[Param::new("seed", ParamType::IntOrString)
                    .nullable()
                    .optional()],
                engine_methods::pcg,
            ),
            MethodEntry::public("generate", &[], engine_methods::generate),
            MethodEntry::public(
                "jump",
                &[Param::new("advance", ParamType::Int)],
                engine_methods::jump,
            ),
        ],
        ..ClassEntry::class("Random\\Engine\\PcgOneseq128XslRr64")
    },
    ClassEntry {
        kind: ClassKind::FinalClass,
        interfaces: &["Random\\Engine"],
        methods: &[
            MethodEntry::public(
                "__construct",
                &[Param::new("seed", ParamType::IntOrString)
                    .nullable()
                    .optional()],
                engine_methods::xoshiro,
            ),
            MethodEntry::public("generate", &[], engine_methods::generate),
            MethodEntry::public("jump", &[], engine_methods::jump),
            MethodEntry::public("jumpLong", &[], engine_methods::jump_long),
        ],
        ..ClassEntry::class("Random\\Engine\\Xoshiro256StarStar")
    },
    ClassEntry {
        kind: ClassKind::FinalClass,
        interfaces: &["Random\\CryptoSafeEngine"],
        methods: &[MethodEntry::public(
            "generate",
            &[],
            engine_methods::generate,
        )],
        ..ClassEntry::class("Random\\Engine\\Secure")
    },
    ClassEntry {
        kind: ClassKind::FinalClass,
        properties: &[PropertyEntry::new(
            "engine",
            ConstantValue::Null,
            Visibility::Public,
        )],
        methods: &[
            MethodEntry::public(
                "__construct",
                &[Param::new("engine", ParamType::Object)
                    .nullable()
                    .optional()],
                randomizer::construct,
            ),
            MethodEntry::public("nextInt", &[], randomizer::next_int),
            MethodEntry::public("nextFloat", &[], randomizer::next_float),
            MethodEntry::public(
                "getInt",
                &[
                    Param::new("min", ParamType::Int),
                    Param::new("max", ParamType::Int),
                ],
                randomizer::get_int,
            ),
            MethodEntry::public(
                "getFloat",
                &[
                    Param::new("min", ParamType::Float),
                    Param::new("max", ParamType::Float),
                    Param::new("boundary", ParamType::Mixed).optional(),
                ],
                randomizer::get_float,
            ),
            MethodEntry::public(
                "getBytes",
                &[Param::new("length", ParamType::Int)],
                randomizer::bytes,
            ),
            MethodEntry::public(
                "getBytesFromString",
                &[
                    Param::new("string", ParamType::String),
                    Param::new("length", ParamType::Int),
                ],
                randomizer::from_string,
            ),
            MethodEntry::public(
                "shuffleArray",
                &[Param::new("array", ParamType::Array)],
                randomizer::shuffle_array,
            ),
            MethodEntry::public(
                "shuffleBytes",
                &[Param::new("bytes", ParamType::String)],
                randomizer::shuffle_bytes,
            ),
            MethodEntry::public(
                "pickArrayKeys",
                &[
                    Param::new("array", ParamType::Array),
                    Param::new("num", ParamType::Int),
                ],
                randomizer::pick_keys,
            ),
        ],
        ..ClassEntry::class("Random\\Randomizer")
    },
];
