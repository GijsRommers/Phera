//! Register attribute, mutation and selector operations for modern elements.
//!
//! ```php
//! $element->setAttribute('class', 'item');
//! var_dump($element->matches('.item'));
//! ```

use phera_extension_api::{
    ClassEntry, ConstantValue as C, MethodEntry as M, Param as P, ParamType as T, Registry,
};

use super::super::super::{attributes, collection, document, mutation};
use super::super::selectors;
use attributes::Read;

pub(in crate::bindings::modern) fn register(registry: &mut Registry) {
    const QUALIFIED: P = P::new("qualifiedName", T::String);
    const LOCAL: P = P::new("localName", T::String);
    const NS: P = P::new("namespace", T::String).nullable();
    const VALUE: P = P::new("value", T::String);
    const ATTR: P = P::new("attr", T::Class("Dom\\Attr"));
    const ID: P = P::new("isId", T::Bool);
    const NODES: P = document::VARIADIC;
    const SELECTORS: P = P::new("selectors", T::String);
    registry.class(ClassEntry {
        parent: Some("Dom\\Node"),
        interfaces: &["Dom\\ParentNode", "Dom\\ChildNode"],
        methods: const {
            &[
                M::public("getAttribute", &[QUALIFIED], |c, t, a| {
                    attributes::read(c, t, a, false, Read::Value)
                })
                .returning("?string"),
                M::public("getAttributeNS", &[NS, LOCAL], |c, t, a| {
                    attributes::read(c, t, a, true, Read::Value)
                })
                .returning("?string"),
                M::public("getAttributeNames", &[], attributes::names).returning("array"),
                M::public("hasAttributes", &[], |_, this, _| {
                    let node = super::super::super::this_node(this)?;
                    let state = node.shared.borrow();
                    Ok(phera_extension_api::Value::Bool(
                        !state.document.arena[node.id].attributes().is_empty(),
                    ))
                })
                .returning("bool"),
                M::public("hasAttribute", &[QUALIFIED], |c, t, a| {
                    attributes::read(c, t, a, false, Read::Has)
                })
                .returning("bool"),
                M::public("hasAttributeNS", &[NS, LOCAL], |c, t, a| {
                    attributes::read(c, t, a, true, Read::Has)
                })
                .returning("bool"),
                M::public("getAttributeNode", &[QUALIFIED], |c, t, a| {
                    attributes::read(c, t, a, false, Read::Node)
                })
                .returning("?Dom\\Attr"),
                M::public("getAttributeNodeNS", &[NS, LOCAL], |c, t, a| {
                    attributes::read(c, t, a, true, Read::Node)
                })
                .returning("?Dom\\Attr"),
                M::public("setAttribute", &[QUALIFIED, VALUE], |c, t, a| {
                    attributes::set_attr(c, t, a, false)
                })
                .returning("void"),
                M::public("setAttributeNS", &[NS, QUALIFIED, VALUE], |c, t, a| {
                    attributes::set_attr(c, t, a, true)
                })
                .returning("void"),
                M::public("setAttributeNode", &[ATTR], |c, t, a| {
                    attributes::set_attribute_node(c, t, a, false)
                })
                .returning("?Dom\\Attr"),
                M::public("setAttributeNodeNS", &[ATTR], |c, t, a| {
                    attributes::set_attribute_node(c, t, a, true)
                })
                .returning("?Dom\\Attr"),
                M::public("removeAttributeNode", &[ATTR], mutation::remove_child)
                    .returning("Dom\\Attr"),
                M::public("removeAttribute", &[QUALIFIED], |c, t, a| {
                    attributes::read(c, t, a, false, Read::Remove)
                })
                .returning("void"),
                M::public("removeAttributeNS", &[NS, LOCAL], |c, t, a| {
                    attributes::read(c, t, a, true, Read::Remove)
                })
                .returning("void"),
                M::public(
                    "toggleAttribute",
                    const {
                        &[
                            QUALIFIED,
                            P::new("force", T::Bool).nullable().with_default(C::Null),
                        ]
                    },
                    attributes::toggle,
                )
                .returning("bool"),
                M::public("setIdAttribute", &[QUALIFIED, ID], |c, t, a| {
                    attributes::read(c, t, a, false, Read::MarkId)
                })
                .returning("void"),
                M::public("setIdAttributeNS", &[NS, QUALIFIED, ID], |c, t, a| {
                    attributes::read(c, t, a, true, Read::MarkId)
                })
                .returning("void"),
                M::public("setIdAttributeNode", &[ATTR, ID], attributes::mark_node)
                    .returning("void"),
                M::public("getElementsByTagName", &[QUALIFIED], collection::tags)
                    .returning("Dom\\HTMLCollection"),
                M::public("getElementsByTagNameNS", &[NS, LOCAL], collection::tags_ns)
                    .returning("Dom\\HTMLCollection"),
                M::public("querySelector", &[SELECTORS], selectors::first)
                    .returning("?Dom\\Element"),
                M::public("querySelectorAll", &[SELECTORS], selectors::all)
                    .returning("Dom\\NodeList"),
                M::public("matches", &[SELECTORS], selectors::matches).returning("bool"),
                M::public("closest", &[SELECTORS], selectors::closest).returning("?Dom\\Element"),
                M::public("append", &[NODES], mutation::append).returning("void"),
                M::public("prepend", &[NODES], mutation::prepend).returning("void"),
                M::public("replaceChildren", &[NODES], mutation::replace_children)
                    .returning("void"),
                M::public("before", &[NODES], mutation::before).returning("void"),
                M::public("after", &[NODES], mutation::after).returning("void"),
                M::public("replaceWith", &[NODES], mutation::replace_with).returning("void"),
                M::public("remove", &[], mutation::remove).returning("void"),
            ]
        },
        ..ClassEntry::class("Dom\\Element")
    });
    registry.class(ClassEntry {
        parent: Some("Dom\\Element"),
        ..ClassEntry::class("Dom\\HTMLElement")
    });
}
