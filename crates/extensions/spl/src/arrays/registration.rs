//! Signatures and native handlers for `new ArrayObject(array: ['answer' => 42])`.
//!
//! ```php
//! $a = new ArrayIterator(array: [1, 2], flags: ArrayIterator::STD_PROP_LIST);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{
    ITERATOR, OBJECT, RECURSIVE, cursor, dimensions, methods, properties, serialization, sorting,
};

const ARRAY: Param = Param::new("array", ParamType::ArrayOrObject);
const FLAGS: Param = Param::new("flags", ParamType::Int);
const KEY: Param = Param::new("key", ParamType::Mixed);
const VALUE: Param = Param::new("value", ParamType::Mixed);
const ITERATOR_CLASS: Param = Param::new("iteratorClass", ParamType::String);
const CALLBACK: Param = Param::new("callback", ParamType::Mixed);
const OFFSET: Param = Param::new("offset", ParamType::Int);
const CONSTANTS: &[(&str, ConstantValue)] = &[
    ("STD_PROP_LIST", ConstantValue::Int(1)),
    ("ARRAY_AS_PROPS", ConstantValue::Int(2)),
];

const ARRAY_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(properties::clone_state)),
    properties: Some(properties::view),
    property_access: Some(&properties::ACCESS),
    dimensions: Some(dimensions::HANDLERS),
    count: Some(methods::native_count),
    ..ClassHooks::EMPTY
};

const ITERATOR_HOOKS: ClassHooks = ClassHooks {
    iterator: Some(cursor::create),
    iterator_by_reference: Some(cursor::create_by_reference),
    ..ARRAY_HOOKS
};

const COMMON: [Method; 20] = const {
    [
        Method::public("offsetExists", &[KEY], dimensions::offset_exists).tentative_return("bool"),
        Method::public("offsetGet", &[KEY], dimensions::offset_get).tentative_return("mixed"),
        Method::public("offsetSet", &[KEY, VALUE], dimensions::offset_set).tentative_return("void"),
        Method::public("offsetUnset", &[KEY], dimensions::offset_unset).tentative_return("void"),
        Method::public_async("append", &[VALUE], methods::append).tentative_return("void"),
        Method::public("getArrayCopy", &[], methods::get_array_copy).tentative_return("array"),
        Method::public("count", &[], methods::count)
            .with_direct(methods::direct_count)
            .tentative_return("int"),
        Method::public("getFlags", &[], methods::get_flags).tentative_return("int"),
        Method::public("setFlags", &[FLAGS], methods::set_flags).tentative_return("void"),
        Method::public_async(
            "asort",
            const { &[FLAGS.with_constant_default("SORT_REGULAR", ConstantValue::Int(0))] },
            sorting::asort,
        )
        .tentative_return("true"),
        Method::public_async(
            "ksort",
            const { &[FLAGS.with_constant_default("SORT_REGULAR", ConstantValue::Int(0))] },
            sorting::ksort,
        )
        .tentative_return("true"),
        Method::public_async("uasort", &[CALLBACK], sorting::uasort).tentative_return("true"),
        Method::public_async("uksort", &[CALLBACK], sorting::uksort).tentative_return("true"),
        Method::public_async("natsort", &[], sorting::natsort).tentative_return("true"),
        Method::public_async("natcasesort", &[], sorting::natcasesort).tentative_return("true"),
        Method::public("serialize", &[], serialization::legacy_serialize)
            .tentative_return("string"),
        Method::public_async(
            "unserialize",
            const { &[Param::new("data", ParamType::String)] },
            serialization::legacy_unserialize,
        )
        .tentative_return("void"),
        Method::public("__serialize", &[], serialization::serialize).tentative_return("array"),
        Method::public_async(
            "__unserialize",
            const { &[Param::new("data", ParamType::Array)] },
            serialization::unserialize,
        )
        .tentative_return("void"),
        Method::public("__debugInfo", &[], methods::debug).tentative_return("array"),
    ]
};

const fn with_common<const LEN: usize, const EXTRA: usize>(
    extra: [Method; EXTRA],
) -> [Method; LEN] {
    let mut methods = [COMMON[0]; LEN];
    let mut index = 0;
    while index < COMMON.len() {
        methods[index] = COMMON[index];
        index += 1;
    }
    let mut extra_index = 0;
    while extra_index < EXTRA {
        methods[index] = extra[extra_index];
        extra_index += 1;
        index += 1;
    }
    methods
}

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &[
            "SeekableIterator",
            "ArrayAccess",
            "Serializable",
            "Countable",
        ],
        constants: CONSTANTS,
        hooks: Some(&ITERATOR_HOOKS),
        methods: const {
            &with_common::<27, 7>([
                Method::public(
                    "__construct",
                    const {
                        &[
                            ARRAY.with_default(ConstantValue::EmptyArray),
                            FLAGS.with_default(ConstantValue::Int(0)),
                        ]
                    },
                    methods::construct_iterator,
                ),
                Method::public("rewind", &[], cursor::rewind).tentative_return("void"),
                Method::public("current", &[], cursor::current).tentative_return("mixed"),
                Method::public("key", &[], cursor::key).tentative_return("string|int|null"),
                Method::public("next", &[], cursor::next).tentative_return("void"),
                Method::public("valid", &[], cursor::valid).tentative_return("bool"),
                Method::public("seek", &[OFFSET], cursor::seek).tentative_return("void"),
            ])
        },
        ..ClassEntry::class(ITERATOR)
    });
    registry.class(ClassEntry {
        interfaces: &[
            "IteratorAggregate",
            "ArrayAccess",
            "Serializable",
            "Countable",
        ],
        constants: CONSTANTS,
        hooks: Some(&ARRAY_HOOKS),
        methods: const {
            &with_common::<25, 5>([
                Method::public_async(
                    "__construct",
                    const {
                        &[
                            ARRAY.with_default(ConstantValue::EmptyArray),
                            FLAGS.with_default(ConstantValue::Int(0)),
                            ITERATOR_CLASS.with_default(ConstantValue::String(ITERATOR)),
                        ]
                    },
                    methods::construct_object,
                ),
                Method::public("getIterator", &[], methods::get_iterator)
                    .tentative_return("Iterator"),
                Method::public("exchangeArray", &[ARRAY], methods::exchange_array)
                    .tentative_return("array"),
                Method::public_async(
                    "setIteratorClass",
                    &[ITERATOR_CLASS],
                    methods::set_iterator_class,
                )
                .with_preparation(methods::prepare_iterator_class)
                .tentative_return("void"),
                Method::public("getIteratorClass", &[], methods::get_iterator_class)
                    .tentative_return("string"),
            ])
        },
        ..ClassEntry::class(OBJECT)
    });
    registry.class(ClassEntry {
        parent: Some(ITERATOR),
        interfaces: &["RecursiveIterator"],
        constants: &[("CHILD_ARRAYS_ONLY", ConstantValue::Int(4))],
        methods: const {
            &[
                Method::public("hasChildren", &[], cursor::has_children).tentative_return("bool"),
                Method::public_async("getChildren", &[], cursor::children)
                    .tentative_return("?RecursiveArrayIterator"),
            ]
        },
        ..ClassEntry::class(RECURSIVE)
    });
}
