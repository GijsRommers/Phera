//! Heap and priority queue hooks share the same binary heap implementation.
//!
//! ```php
//! $queue->setExtractFlags(SplPriorityQueue::EXTR_BOTH);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, MethodEntry as Method, NativeClone, Param,
    ParamType, Registry, Visibility,
};

use super::{HEAP, PRIORITY_QUEUE, operations as heap, views};

const VALUE: Param = Param::new("value", ParamType::Mixed);
const PRIORITY: Param = Param::new("priority", ParamType::Mixed);
const DATA: Param = Param::new("data", ParamType::Array);
const FLAGS: Param = Param::new("flags", ParamType::Int);
const HEAP_COMPARE: &[Param] = &[
    Param::new("value1", ParamType::Mixed),
    Param::new("value2", ParamType::Mixed),
];
const QUEUE_COMPARE: &[Param] = &[
    Param::new("priority1", ParamType::Mixed),
    Param::new("priority2", ParamType::Mixed),
];

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(views::clone_heap)),
    properties: Some(views::view),
    iterator: Some(heap::cursor),
    iterator_fallback_if_overridden: &["compare"],
    count: Some(heap::native_count),
    ..ClassHooks::EMPTY
};

const COMMON: [Method; 14] = const {
    [
        Method::public_async("extract", &[], heap::extract).tentative_return("mixed"),
        Method::public("top", &[], heap::top).tentative_return("mixed"),
        Method::public("count", &[], heap::count).tentative_return("int"),
        Method::public("isEmpty", &[], heap::is_empty).tentative_return("bool"),
        Method::public("rewind", &[], heap::rewind).tentative_return("void"),
        Method::public("current", &[], heap::current).tentative_return("mixed"),
        Method::public("key", &[], heap::key).tentative_return("int"),
        Method::public_async("next", &[], heap::next).tentative_return("void"),
        Method::public("valid", &[], heap::valid).tentative_return("bool"),
        Method::public("recoverFromCorruption", &[], heap::recover).tentative_return("true"),
        Method::public("isCorrupted", &[], heap::corrupted).tentative_return("bool"),
        Method::public("__debugInfo", &[], views::debug_info).tentative_return("array"),
        Method::public("__serialize", &[], views::serialize).tentative_return("array"),
        Method::public_async("__unserialize", &[DATA], views::unserialize).tentative_return("void"),
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
        index += 1;
        extra_index += 1;
    }
    methods
}

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::AbstractClass,
        interfaces: &["Iterator", "Countable"],
        hooks: Some(&HOOKS),
        methods: const {
            &with_common::<16, 2>([
                Method::public_async("insert", &[VALUE], heap::insert).tentative_return("true"),
                Method::required("compare", HEAP_COMPARE)
                    .with_visibility(Visibility::Protected)
                    .tentative_return("int"),
            ])
        },
        ..ClassEntry::class(HEAP)
    });
    registry.class(ClassEntry {
        parent: Some(HEAP),
        methods: const {
            &[Method::public("compare", HEAP_COMPARE, heap::compare_min)
                .with_visibility(Visibility::Protected)
                .tentative_return("int")]
        },
        ..ClassEntry::class("SplMinHeap")
    });
    registry.class(ClassEntry {
        parent: Some(HEAP),
        methods: const {
            &[Method::public("compare", HEAP_COMPARE, heap::compare_max)
                .with_visibility(Visibility::Protected)
                .tentative_return("int")]
        },
        ..ClassEntry::class("SplMaxHeap")
    });
    registry.class(ClassEntry {
        interfaces: &["Iterator", "Countable"],
        constants: &[
            ("EXTR_DATA", ConstantValue::Int(1)),
            ("EXTR_PRIORITY", ConstantValue::Int(2)),
            ("EXTR_BOTH", ConstantValue::Int(3)),
        ],
        hooks: Some(&HOOKS),
        methods: const {
            &with_common::<18, 4>([
                Method::public_async("insert", &[VALUE, PRIORITY], heap::insert)
                    .tentative_return("true"),
                Method::public("compare", QUEUE_COMPARE, heap::compare_max).tentative_return("int"),
                Method::public("setExtractFlags", &[FLAGS], heap::set_flags)
                    .tentative_return("int"),
                Method::public("getExtractFlags", &[], heap::get_flags).tentative_return("int"),
            ])
        },
        ..ClassEntry::class(PRIORITY_QUEUE)
    });
}
