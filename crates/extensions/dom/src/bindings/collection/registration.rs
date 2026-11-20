//! Register the PHP hooks and methods of the DOM collection classes.
//!
//! ```php
//! foreach ($root->attributes as $name => $attribute) echo $name;
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, MethodEntry, NativeDimensions, NativePropertyAccess, Param, ParamType,
    Registry,
};

use super::{
    count, count_hook, cursor, dimensions, item, mutate, named, named_ns, projection, read,
};

const HOOKS: ClassHooks = ClassHooks {
    iterator: Some(cursor::iterator),
    properties: Some(projection),
    property_access: Some(&NativePropertyAccess { read, mutate }),
    count: Some(count_hook),
    dimensions: Some(NativeDimensions {
        fallback_if_overridden: &[],
        get: dimensions::get,
        exists: dimensions::exists,
        set: dimensions::set,
        unset: dimensions::unset,
        reference: None,
        bind: None,
        direct: None,
    }),
    ..ClassHooks::EMPTY
};

pub(in crate::bindings) fn register(registry: &mut Registry) {
    const ITEM: MethodEntry = MethodEntry::public(
        "item",
        const { &[Param::new("index", ParamType::Int)] },
        item,
    );
    const COUNT: MethodEntry = MethodEntry::public("count", &[], count).tentative_return("int");
    const ITERATOR: MethodEntry =
        MethodEntry::public("getIterator", &[], cursor::get_iterator).returning("Iterator");
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate", "Countable"],
        methods: const { &[ITEM, COUNT, ITERATOR] },
        hooks: Some(&HOOKS),
        ..ClassEntry::class("DOMNodeList")
    });
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate", "Countable"],
        methods: const {
            const {
                &[
                    ITEM.tentative_return("?DOMNode"),
                    COUNT,
                    ITERATOR,
                    MethodEntry::public(
                        "getNamedItem",
                        const { &[Param::new("qualifiedName", ParamType::String)] },
                        named,
                    )
                    .tentative_return("?DOMNode"),
                    MethodEntry::public(
                        "getNamedItemNS",
                        const {
                            &[
                                Param::new("namespace", ParamType::String).nullable(),
                                Param::new("localName", ParamType::String),
                            ]
                        },
                        named_ns,
                    )
                    .tentative_return("?DOMNode"),
                ]
            }
        },
        hooks: Some(&HOOKS),
        ..ClassEntry::class("DOMNamedNodeMap")
    });
    // `Dom\HTMLCollection` looks an element up by its `id` or `name` attribute.
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate", "Countable"],
        methods: const {
            &[
                ITEM.returning("?Dom\\Element"),
                MethodEntry::public(
                    "namedItem",
                    const { &[Param::new("key", ParamType::String)] },
                    named,
                )
                .returning("?Dom\\Element"),
                COUNT.returning("int"),
                ITERATOR,
            ]
        },
        hooks: Some(&HOOKS),
        ..ClassEntry::class("Dom\\HTMLCollection")
    });
}
