//! Declare the signatures of element and attribute methods.
//!
//! ```php
//! $element->setAttributeNS(namespace: 'urn:x', qualifiedName: 'x:id', value: '1');
//! ```

use phera_extension_api::{
    ClassEntry, ConstantValue as C, MethodEntry as M, Param as P, ParamType as T, Registry,
};

use super::super::{collection, document::VARIADIC};
use super::*;

pub(in crate::bindings) fn register(registry: &mut Registry) {
    const QUALIFIED: P = P::new("qualifiedName", T::String);
    const NS: P = P::new("namespace", T::String).nullable();
    const LOCAL: P = P::new("localName", T::String);
    const VALUE: P = P::new("value", T::String);
    const ATTR: P = P::new("attr", T::Class("DOMAttr"));
    const IS_ID: P = P::new("isId", T::Bool);

    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        methods: const {
            const {
                &[
                    M::public(
                        "__construct",
                        const { &[P::new("name", T::String), VALUE.with_default(C::String(""))] },
                        factory::construct_attribute,
                    ),
                    M::public("isId", &[], is_id).tentative_return("bool"),
                ]
            }
        },
        ..ClassEntry::class("DOMAttr")
    });

    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        interfaces: &["DOMParentNode", "DOMChildNode"],
        methods: const {
            const {
                &[
                    M::public(
                        "__construct",
                        const {
                            &[
                                QUALIFIED,
                                VALUE.nullable().with_default(C::Null),
                                P::new("namespace", T::String).with_default(C::String("")),
                            ]
                        },
                        factory::construct_element,
                    ),
                    M::public("getAttribute", &[QUALIFIED], |c, t, a| {
                        read(c, t, a, false, Read::Value)
                    })
                    .tentative_return("string"),
                    M::public("getAttributeNS", &[NS, LOCAL], |c, t, a| {
                        read(c, t, a, true, Read::Value)
                    })
                    .tentative_return("string"),
                    M::public("hasAttribute", &[QUALIFIED], |c, t, a| {
                        read(c, t, a, false, Read::Has)
                    })
                    .tentative_return("bool"),
                    M::public("hasAttributeNS", &[NS, LOCAL], |c, t, a| {
                        read(c, t, a, true, Read::Has)
                    })
                    .tentative_return("bool"),
                    M::public("getAttributeNode", &[QUALIFIED], |c, t, a| {
                        read(c, t, a, false, Read::Node)
                    }),
                    M::public("getAttributeNodeNS", &[NS, LOCAL], |c, t, a| {
                        read(c, t, a, true, Read::Node)
                    }),
                    M::public("removeAttribute", &[QUALIFIED], |c, t, a| {
                        read(c, t, a, false, Read::Remove)
                    })
                    .tentative_return("bool"),
                    M::public("removeAttributeNS", &[NS, LOCAL], |c, t, a| {
                        read(c, t, a, true, Read::Remove)
                    })
                    .tentative_return("void"),
                    M::public("setAttribute", &[QUALIFIED, VALUE], |c, t, a| {
                        set_attr(c, t, a, false)
                    }),
                    M::public("setAttributeNS", &[NS, QUALIFIED, VALUE], |c, t, a| {
                        set_attr(c, t, a, true)
                    })
                    .tentative_return("void"),
                    M::public("getAttributeNames", &[], names).returning("array"),
                    M::public("setIdAttribute", &[QUALIFIED, IS_ID], |c, t, a| {
                        read(c, t, a, false, Read::MarkId)
                    })
                    .tentative_return("void"),
                    M::public(
                        "setIdAttributeNS",
                        const { &[P::new("namespace", T::String), QUALIFIED, IS_ID] },
                        |c, t, a| read(c, t, a, true, Read::MarkId),
                    )
                    .tentative_return("void"),
                    M::public("setIdAttributeNode", &[ATTR, IS_ID], mark_node)
                        .tentative_return("void"),
                    M::public("setAttributeNode", &[ATTR], |c, t, a| {
                        set_attribute_node(c, t, a, false)
                    }),
                    M::public("setAttributeNodeNS", &[ATTR], |c, t, a| {
                        set_attribute_node(c, t, a, true)
                    }),
                    M::public("removeAttributeNode", &[ATTR], mutation::remove_child),
                    M::public(
                        "toggleAttribute",
                        const {
                            &[
                                QUALIFIED,
                                P::new("force", T::Bool).nullable().with_default(C::Null),
                            ]
                        },
                        toggle,
                    )
                    .returning("bool"),
                    M::public("getElementsByTagName", &[QUALIFIED], collection::tags)
                        .tentative_return("DOMNodeList"),
                    M::public("getElementsByTagNameNS", &[NS, LOCAL], collection::tags_ns)
                        .tentative_return("DOMNodeList"),
                    M::public("append", &[VARIADIC], mutation::append).returning("void"),
                    M::public("prepend", &[VARIADIC], mutation::prepend).returning("void"),
                    M::public("replaceChildren", &[VARIADIC], mutation::replace_children)
                        .returning("void"),
                    M::public("before", &[VARIADIC], mutation::before).returning("void"),
                    M::public("after", &[VARIADIC], mutation::after).returning("void"),
                    M::public("replaceWith", &[VARIADIC], mutation::replace_with).returning("void"),
                    M::public("remove", &[], mutation::remove).returning("void"),
                    M::public(
                        "insertAdjacentElement",
                        const {
                            &[
                                P::new("where", T::String),
                                P::new("element", T::Class("DOMElement")),
                            ]
                        },
                        mutation::insert_adjacent_element,
                    )
                    .returning("?DOMElement"),
                    M::public(
                        "insertAdjacentText",
                        const { &[P::new("where", T::String), P::new("data", T::String)] },
                        mutation::insert_adjacent_text,
                    )
                    .returning("void"),
                ]
            }
        },
        ..ClassEntry::class("DOMElement")
    });
}
