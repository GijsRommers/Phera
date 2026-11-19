//! SPL's list, queue and stack share the same methods and native storage.
//!
//! ```php
//! $q = new SplQueue;
//! $q->setIteratorMode(SplQueue::IT_MODE_FIFO | SplQueue::IT_MODE_DELETE);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{CLASS, cursor, dimensions, methods};

const INDEX: Param = Param::new("index", ParamType::Mixed);
const INDEX_INT: Param = Param::new("index", ParamType::Int);
const VALUE: Param = Param::new("value", ParamType::Mixed);
const MODE: Param = Param::new("mode", ParamType::Int);
const DATA: Param = Param::new("data", ParamType::Array);
const STRING_DATA: Param = Param::new("data", ParamType::String);

const HOOKS: ClassHooks = ClassHooks {
    fork: Some(crate::native_state::fork_spl),
    clone: Some(NativeClone::Copy(methods::clone_list)),
    properties: Some(methods::view),
    iterator: Some(cursor::create),
    dimensions: Some(dimensions::OPERATIONS),
    count: Some(methods::native_count),
    ..ClassHooks::EMPTY
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Iterator", "Countable", "ArrayAccess", "Serializable"],
        constants: &[
            ("IT_MODE_LIFO", ConstantValue::Int(2)),
            ("IT_MODE_FIFO", ConstantValue::Int(0)),
            ("IT_MODE_DELETE", ConstantValue::Int(1)),
            ("IT_MODE_KEEP", ConstantValue::Int(0)),
        ],
        hooks: Some(&HOOKS),
        methods: const {
            &[
                Method::public("add", &[INDEX_INT, VALUE], methods::add).tentative_return("void"),
                Method::public("pop", &[], methods::pop)
                    .with_direct(methods::pop_direct)
                    .tentative_return("mixed"),
                Method::public("shift", &[], methods::shift)
                    .with_direct(methods::shift_direct)
                    .tentative_return("mixed"),
                Method::public("push", &[VALUE], methods::push)
                    .with_direct(methods::push_direct)
                    .tentative_return("void"),
                Method::public("unshift", &[VALUE], methods::unshift).tentative_return("void"),
                Method::public("top", &[], methods::top).tentative_return("mixed"),
                Method::public("bottom", &[], methods::bottom).tentative_return("mixed"),
                Method::public("__debugInfo", &[], methods::debug_info).tentative_return("array"),
                Method::public("count", &[], methods::count).tentative_return("int"),
                Method::public("isEmpty", &[], methods::is_empty).tentative_return("bool"),
                Method::public("setIteratorMode", &[MODE], methods::set_mode)
                    .tentative_return("int"),
                Method::public("getIteratorMode", &[], methods::get_mode).tentative_return("int"),
                Method::public("offsetExists", &[INDEX], dimensions::offset_exists)
                    .tentative_return("bool"),
                Method::public("offsetGet", &[INDEX], dimensions::offset_get)
                    .tentative_return("mixed"),
                Method::public("offsetSet", &[INDEX, VALUE], dimensions::offset_set)
                    .tentative_return("void"),
                Method::public("offsetUnset", &[INDEX], dimensions::offset_unset)
                    .tentative_return("void"),
                Method::public("rewind", &[], cursor::rewind).tentative_return("void"),
                Method::public("current", &[], cursor::current).tentative_return("mixed"),
                Method::public("key", &[], cursor::key).tentative_return("int"),
                Method::public("prev", &[], cursor::prev).tentative_return("void"),
                Method::public("next", &[], cursor::next).tentative_return("void"),
                Method::public("valid", &[], cursor::valid).tentative_return("bool"),
                Method::public("unserialize", &[STRING_DATA], methods::legacy_unserialize)
                    .tentative_return("void"),
                Method::public("serialize", &[], methods::legacy_serialize)
                    .tentative_return("string"),
                Method::public("__serialize", &[], methods::serialize).tentative_return("array"),
                Method::public("__unserialize", &[DATA], methods::unserialize)
                    .tentative_return("void"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        methods: const {
            &[
                Method::public("enqueue", &[VALUE], methods::push)
                    .with_direct(methods::push_direct)
                    .tentative_return("void"),
                Method::public("dequeue", &[], methods::shift)
                    .with_direct(methods::shift_direct)
                    .tentative_return("mixed"),
            ]
        },
        ..ClassEntry::class("SplQueue")
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        ..ClassEntry::class("SplStack")
    });
}
