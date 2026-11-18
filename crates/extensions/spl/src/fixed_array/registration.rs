//! The fixed array exposes ArrayAccess, Countable, JsonSerializable and IteratorAggregate.
//!
//! ```php
//! $a = SplFixedArray::fromArray([2 => 'x']);
//! echo $a->getSize(); // 3
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{CLASS, dimensions, iteration};

const SIZE: Param = Param::new("size", ParamType::Int);
const KEY: Param = Param::new("index", ParamType::Mixed);
const VALUE: Param = Param::new("value", ParamType::Mixed);
const ARRAY: Param = Param::new("array", ParamType::Array);
const DATA: Param = Param::new("data", ParamType::Array);
const PRESERVE: Param =
    Param::new("preserveKeys", ParamType::Bool).with_default(ConstantValue::Bool(true));

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(super::clone_array)),
    properties: Some(super::view),
    iterator: Some(iteration::cursor),
    dimensions: Some(dimensions::OPERATIONS),
    count: Some(super::native_count),
    ..ClassHooks::EMPTY
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: Some(&HOOKS),
        interfaces: &[
            "IteratorAggregate",
            "ArrayAccess",
            "Countable",
            "JsonSerializable",
        ],
        methods: const {
            &[
                Method::public(
                    "__construct",
                    const { &[SIZE.with_default(ConstantValue::Int(0))] },
                    super::construct,
                ),
                Method::public("__serialize", &[], super::serialize).returning("array"),
                Method::public("__unserialize", &[DATA], super::unserialize).returning("void"),
                Method::public("count", &[], super::count).tentative_return("int"),
                Method::public("getSize", &[], super::count).tentative_return("int"),
                Method::public("setSize", &[SIZE], super::set_size).tentative_return("true"),
                Method::public("toArray", &[], super::to_array).tentative_return("array"),
                Method::public("fromArray", &[ARRAY, PRESERVE], super::from_array)
                    .static_()
                    .tentative_return(CLASS),
                Method::public("offsetExists", &[KEY], dimensions::offset_exists)
                    .tentative_return("bool"),
                Method::public("offsetGet", &[KEY], dimensions::offset_get)
                    .tentative_return("mixed"),
                Method::public("offsetSet", &[KEY, VALUE], dimensions::offset_set)
                    .tentative_return("void"),
                Method::public("offsetUnset", &[KEY], dimensions::offset_unset)
                    .tentative_return("void"),
                Method::public("getIterator", &[], iteration::get_iterator).returning("Iterator"),
                Method::public("jsonSerialize", &[], super::to_array).returning("array"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
}
