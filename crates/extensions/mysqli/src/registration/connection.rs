//! PHP signatures for mysqli.

mod functions0;
mod functions1;
mod functions2;
mod functions3;
mod methods0;
mod methods1;
mod methods2;

use super::MYSQLI_HOOKS;
use phera_extension_api::{ClassEntry, MethodEntry, Registry};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: &METHODS,
        hooks: Some(&MYSQLI_HOOKS),
        ..ClassEntry::class("mysqli")
    });
    for &entry in functions0::ENTRIES {
        registry.function(entry);
    }
    for &entry in functions1::ENTRIES {
        registry.function(entry);
    }
    for &entry in functions2::ENTRIES {
        registry.function(entry);
    }
    for &entry in functions3::ENTRIES {
        registry.function(entry);
    }
}

const METHODS: [MethodEntry; 35] = {
    let mut methods = [methods0::ENTRIES[0]; 35];
    let mut index = 0;
    let groups = [methods0::ENTRIES, methods1::ENTRIES, methods2::ENTRIES];
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
