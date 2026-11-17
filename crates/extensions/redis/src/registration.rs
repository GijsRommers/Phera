//! `Redis` and `RedisException` only; cluster, sentinel and array are separate APIs.
use phera_extension_api::{ClassEntry, Registry};
mod group_00;
mod group_01;
mod group_02;
mod group_03;
mod group_04;
mod group_05;
mod group_06;
mod group_07;
mod group_08;
mod group_09;
mod group_10;
mod group_11;
mod group_12;
mod group_13;
mod group_14;
mod group_15;
mod group_16;
mod group_17;
mod group_18;
mod group_19;
mod group_20;
mod group_21;
mod group_22;
mod group_23;
mod group_24;
mod group_25;
mod group_26;
mod group_27;
mod group_28;
pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("RuntimeException"),
        ..ClassEntry::class("RedisException")
    });
    registry.class(ClassEntry {
        methods: METHODS,
        constants: CONSTANTS,
        ..ClassEntry::class("Redis")
    });
}

mod constants;
mod methods;
use constants::CONSTANTS;
use methods::METHODS;
