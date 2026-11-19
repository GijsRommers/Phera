//! PHP signatures for mysqli_result.

mod functions0;
mod methods0;

use crate::result;
use phera_extension_api::{ClassEntry, MethodEntry, Registry};
use phera_extension_api::{ClassHooks, NativeClone};

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    property_access: Some(&result::ACCESS),
    iterator: Some(result::iterator),
    ..ClassHooks::EMPTY
};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: &METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class("mysqli_result")
    });
    for &entry in functions0::ENTRIES {
        registry.function(entry);
    }
}

const METHODS: [MethodEntry; 15] = {
    let mut methods = [methods0::ENTRIES[0]; 15];
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
