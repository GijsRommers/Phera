//! Methods and fast paths for `new SplObjectStorage`.
//!
//! ```php
//! $s = new SplObjectStorage;
//! $s->offsetSet(new stdClass, 'record');
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{CLASS, dimensions, iteration, views};

const OBJECT: Param = Param::new("object", ParamType::Object);
const OFFSET: Param = Param::new("object", ParamType::Mixed);
const INFO: Param = Param::new("info", ParamType::Mixed);
const STORAGE: Param = Param::new("storage", ParamType::Class(CLASS));
const DATA: Param = Param::new("data", ParamType::Array);
const STRING_DATA: Param = Param::new("data", ParamType::String);
const MODE: Param = Param::new("mode", ParamType::Int).with_default(ConstantValue::Int(0));

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &[
            "Countable",
            "SeekableIterator",
            "Serializable",
            "ArrayAccess",
        ],
        hooks: Some(&ClassHooks {
            clone: Some(NativeClone::Copy(iteration::clone_storage)),
            compare: Some(views::compare),
            properties: Some(views::view),
            iterator: Some(iteration::create),
            dimensions: Some(dimensions::HANDLERS),
            count: Some(iteration::native_count),
            ..ClassHooks::EMPTY
        }),
        methods: const {
            &[
                Method::public_async(
                    "attach",
                    const { &[OFFSET, INFO.with_default(ConstantValue::Null)] },
                    dimensions::attach_old,
                )
                .tentative_return("void"),
                Method::public_async("detach", &[OFFSET], dimensions::detach_old)
                    .tentative_return("void"),
                Method::public_async("contains", &[OFFSET], dimensions::contains_old)
                    .tentative_return("bool"),
                Method::public_async("addAll", &[STORAGE], iteration::add_all)
                    .tentative_return("int"),
                Method::public_async("removeAll", &[STORAGE], iteration::remove_all)
                    .tentative_return("int"),
                Method::public_async("removeAllExcept", &[STORAGE], iteration::remove_all_except)
                    .tentative_return("int"),
                Method::public("getInfo", &[], iteration::get_info).tentative_return("mixed"),
                Method::public("setInfo", &[INFO], iteration::set_info).tentative_return("void"),
                Method::public("count", &[MODE], iteration::count).tentative_return("int"),
                Method::public("rewind", &[], iteration::rewind).tentative_return("void"),
                Method::public("valid", &[], iteration::valid).tentative_return("bool"),
                Method::public("key", &[], iteration::key).tentative_return("int"),
                Method::public("current", &[], iteration::current).tentative_return("object"),
                Method::public("next", &[], iteration::next).tentative_return("void"),
                Method::public(
                    "seek",
                    const { &[Param::new("offset", ParamType::Int)] },
                    iteration::seek,
                )
                .returning("void"),
                Method::public("unserialize", &[STRING_DATA], views::legacy)
                    .tentative_return("void"),
                Method::public("serialize", &[], views::legacy).tentative_return("string"),
                Method::public_async("offsetExists", &[OFFSET], dimensions::offset_exists)
                    .tentative_return("bool"),
                Method::public_async("offsetGet", &[OFFSET], dimensions::offset_get)
                    .tentative_return("mixed"),
                Method::public_async(
                    "offsetSet",
                    const { &[OFFSET, INFO.with_default(ConstantValue::Null)] },
                    dimensions::offset_set,
                )
                .tentative_return("void"),
                Method::public_async("offsetUnset", &[OFFSET], dimensions::offset_unset)
                    .tentative_return("void"),
                Method::public("getHash", &[OBJECT], iteration::get_hash)
                    .tentative_return("string"),
                Method::public("__serialize", &[], views::serialize).tentative_return("array"),
                Method::public_async("__unserialize", &[DATA], views::unserialize)
                    .tentative_return("void"),
                Method::public("__debugInfo", &[], views::debug_info).tentative_return("array"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
}
