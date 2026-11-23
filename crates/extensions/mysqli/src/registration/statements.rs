//! PHP signatures for mysqli_stmt.

mod functions0;
mod functions1;
mod methods0;

use crate::statements;
use phera_extension_api::{ClassEntry, FunctionEntry, MethodEntry, Param, ParamType, Registry};
use phera_extension_api::{ClassHooks, NativeClone};

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    fork: Some(phera_extension_api::fork_refused_handle),
    property_access: Some(&statements::properties::ACCESS),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: &METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class("mysqli_stmt")
    });
    for &entry in functions0::ENTRIES {
        registry.function(entry);
    }
    for &entry in functions1::ENTRIES {
        registry.function(entry);
    }
    registry.function(FunctionEntry::asynchronous(
        "mysqli_stmt_get_warnings",
        const { &[Param::new("statement", ParamType::Class("mysqli_stmt"))] },
        super::statement_dispatch::warnings_function,
    ));
    registry.describe_function("mysqli_stmt_get_warnings", "mysqli_warning|false", None);
}

const METHODS: [MethodEntry; 18] = {
    let mut methods = [methods0::ENTRIES[0]; 18];
    let mut index = 0;
    let groups = [methods0::ENTRIES];
    let mut group = 0;
    while group < groups.len() {
        let mut item = 0;
        while item < groups[group].len() {
            methods[index] = groups[group][item];
            index += 1;
            item += 1;
        }
        group += 1;
    }
    methods
};
