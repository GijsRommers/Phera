//! Declare character data, document fragment, and processing instruction classes.
//!
//! ```php
//! $document->createDocumentFragment()->appendXML('<a/><b/>');
//! ```

use phera_extension_api::{
    ClassEntry, ConstantValue as C, MethodEntry as M, Param as P, ParamType as T, Registry,
};

use super::super::{document::VARIADIC, factory};
use super::*;

pub(in crate::bindings) fn register(registry: &mut Registry) {
    const DATA: P = P::new("data", T::String);
    const OFFSET: P = P::new("offset", T::Int);
    const COUNT: P = P::new("count", T::Int);
    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        interfaces: &["DOMChildNode"],
        methods: const {
            &[
                M::public("appendData", &[DATA], |c, t, a| {
                    change(c, t, a, Operation::Append)
                })
                .tentative_return("true"),
                M::public("substringData", &[OFFSET, COUNT], |c, t, a| {
                    change(c, t, a, Operation::Substring)
                }),
                M::public("insertData", &[OFFSET, DATA], |c, t, a| {
                    change(c, t, a, Operation::Insert)
                })
                .tentative_return("bool"),
                M::public("deleteData", &[OFFSET, COUNT], |c, t, a| {
                    change(c, t, a, Operation::Delete)
                })
                .tentative_return("bool"),
                M::public("replaceData", &[OFFSET, COUNT, DATA], |c, t, a| {
                    change(c, t, a, Operation::Replace)
                })
                .tentative_return("bool"),
                M::public("before", &[VARIADIC], mutation::before).returning("void"),
                M::public("after", &[VARIADIC], mutation::after).returning("void"),
                M::public("replaceWith", &[VARIADIC], mutation::replace_with).returning("void"),
                M::public("remove", &[], mutation::remove).returning("void"),
            ]
        },
        ..ClassEntry::class("DOMCharacterData")
    });
    registry.class(ClassEntry {
        parent: Some("DOMCharacterData"),
        methods: const {
            const {
                &[
                    M::public(
                        "__construct",
                        const { &[DATA.with_default(C::String(""))] },
                        factory::construct_text,
                    ),
                    M::public("splitText", &[OFFSET], split),
                    M::public("isWhitespaceInElementContent", &[], whitespace)
                        .tentative_return("bool"),
                    M::public("isElementContentWhitespace", &[], whitespace)
                        .tentative_return("bool"),
                ]
            }
        },
        ..ClassEntry::class("DOMText")
    });
    registry.class(ClassEntry {
        parent: Some("DOMText"),
        methods: const { &[M::public("__construct", &[DATA], factory::construct_cdata)] },
        ..ClassEntry::class("DOMCdataSection")
    });
    registry.class(ClassEntry {
        parent: Some("DOMCharacterData"),
        methods: const {
            const {
                &[M::public(
                    "__construct",
                    const { &[DATA.with_default(C::String(""))] },
                    factory::construct_comment,
                )]
            }
        },
        ..ClassEntry::class("DOMComment")
    });
    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        methods: const {
            const {
                &[M::public(
                    "__construct",
                    const {
                        &[
                            P::new("name", T::String),
                            P::new("value", T::String).with_default(C::String("")),
                        ]
                    },
                    factory::construct_pi,
                )]
            }
        },
        ..ClassEntry::class("DOMProcessingInstruction")
    });
    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        methods: const {
            const {
                &[M::public(
                    "__construct",
                    const { &[P::new("name", T::String)] },
                    factory::construct_entity,
                )]
            }
        },
        ..ClassEntry::class("DOMEntityReference")
    });
    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        interfaces: &["DOMParentNode"],
        methods: const {
            &[
                M::public("__construct", &[], factory::construct_fragment),
                M::public(
                    "appendXML",
                    const { &[P::new("data", T::String)] },
                    append_xml,
                )
                .tentative_return("bool"),
                M::public("append", &[VARIADIC], mutation::append).returning("void"),
                M::public("prepend", &[VARIADIC], mutation::prepend).returning("void"),
                M::public("replaceChildren", &[VARIADIC], mutation::replace_children)
                    .returning("void"),
            ]
        },
        ..ClassEntry::class("DOMDocumentFragment")
    });
}
