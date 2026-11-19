//! `Reflection::getModifierNames(ReflectionMethod::IS_STATIC)` expands modifier flags.

use phera_extension_api::{ClassEntry, MethodEntry, Param, ParamType, Registry, Value, args};

use crate::values;

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: const {
            &[MethodEntry::public(
                "getModifierNames",
                const { &[Param::new("modifiers", ParamType::Int)] },
                |_, _, args| {
                    let bits = args::int(args, 0).unwrap_or_default();
                    let flags = [
                        (bits & 64 != 0, "abstract"),
                        (bits & 32 != 0, "final"),
                        (bits & 512 != 0, "virtual"),
                        (bits & 7 == 1, "public"),
                        (bits & 7 == 2, "protected"),
                        (bits & 7 == 4, "private"),
                        (bits & 6144 == 2048, "protected(set)"),
                        (bits & 6144 == 4096, "private(set)"),
                        (bits & 16 != 0, "static"),
                        (bits & (128 | 65536) != 0, "readonly"),
                    ];
                    values(
                        flags
                            .into_iter()
                            .filter(|(present, _)| *present)
                            .map(|(_, name)| Value::from(name)),
                    )
                },
            )
            .static_()
            .tentative_return("array")]
        },
        ..ClassEntry::class("Reflection")
    });
}
