//! Internal PostgreSQL handles such as `PgSql\Connection` cannot be constructed directly.

mod aliases;
mod commands0;
mod connection0;
mod query0;
mod result0;
mod result1;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, MethodEntry, NativeClone, NativeError,
    NativeResult, Object, Registry, Value, Visibility,
};
use std::rc::Rc;

pub fn register(registry: &mut Registry) {
    for class in ["PgSql\\Connection", "PgSql\\Result", "PgSql\\Lob"] {
        registry.class(ClassEntry {
            kind: ClassKind::FinalClass,
            methods: &METHODS,
            hooks: Some(&HOOKS),
            ..ClassEntry::class(class)
        });
    }
    let groups = [
        connection0::ENTRIES,
        query0::ENTRIES,
        result0::ENTRIES,
        result1::ENTRIES,
        commands0::ENTRIES,
    ];
    for entries in groups {
        for &entry in entries {
            registry.function(entry);
        }
    }
    aliases::register(registry, groups.into_iter().flatten());
}

const METHODS: [MethodEntry; 1] =
    [MethodEntry::public("__construct", &[], construct).with_visibility(Visibility::Private)];

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    ..ClassHooks::EMPTY
};

fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct a PostgreSQL handle",
    ))
}
