//! Caching iterator flags control lookahead, string conversion and full-cache access.
//!
//! ```php
//! $c = new CachingIterator($iterator, CachingIterator::TOSTRING_USE_KEY);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, Param, ParamType, Registry,
};

use super::{CLASS, RECURSIVE, dimensions};

const ITERATOR: Param = Param::new("iterator", ParamType::Class("Iterator"));
const CHILD: Param = Param::new("iterator", ParamType::Class("RecursiveIterator"));
const FLAGS: Param = Param::new("flags", ParamType::Int);
const KEY: Param = Param::new("key", ParamType::Mixed);
const VALUE: Param = Param::new("value", ParamType::Mixed);

const HOOKS: ClassHooks = ClassHooks {
    dimensions: Some(dimensions::DIMENSIONS),
    count: Some(dimensions::native_count),
    ..ClassHooks::EMPTY
};

pub(in crate::wrappers) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("IteratorIterator"),
        interfaces: &["ArrayAccess", "Countable", "Stringable"],
        hooks: Some(&HOOKS),
        constants: &[
            ("CALL_TOSTRING", ConstantValue::Int(1)),
            ("CATCH_GET_CHILD", ConstantValue::Int(16)),
            ("TOSTRING_USE_KEY", ConstantValue::Int(2)),
            ("TOSTRING_USE_CURRENT", ConstantValue::Int(4)),
            ("TOSTRING_USE_INNER", ConstantValue::Int(8)),
            ("FULL_CACHE", ConstantValue::Int(256)),
        ],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &[ITERATOR, FLAGS.with_default(ConstantValue::Int(1))] },
                    super::construct,
                ),
                Method::public_async("rewind", &[], super::on_rewind).tentative_return("void"),
                Method::public("valid", &[], crate::wrappers::methods::valid)
                    .tentative_return("bool"),
                Method::public_async("next", &[], super::on_next).tentative_return("void"),
                Method::public_async("hasNext", &[], super::has_next).tentative_return("bool"),
                Method::public_async("__toString", &[], super::as_string).returning("string"),
                Method::public("getFlags", &[], super::get_flags).tentative_return("int"),
                Method::public("setFlags", &[FLAGS], super::set_flags).tentative_return("void"),
                Method::public("offsetGet", &[KEY], dimensions::offset_get)
                    .tentative_return("mixed"),
                Method::public("offsetSet", &[KEY, VALUE], dimensions::offset_set)
                    .tentative_return("void"),
                Method::public("offsetUnset", &[KEY], dimensions::offset_unset)
                    .tentative_return("void"),
                Method::public("offsetExists", &[KEY], dimensions::offset_exists)
                    .tentative_return("bool"),
                Method::public("getCache", &[], dimensions::get_cache).tentative_return("array"),
                Method::public("count", &[], dimensions::count).tentative_return("int"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        interfaces: &["RecursiveIterator"],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &[CHILD, FLAGS.with_default(ConstantValue::Int(1))] },
                    super::construct,
                ),
                Method::public("hasChildren", &[], super::has_children).tentative_return("bool"),
                Method::public("getChildren", &[], super::get_children)
                    .tentative_return("?RecursiveCachingIterator"),
            ]
        },
        ..ClassEntry::class(RECURSIVE)
    });
}
