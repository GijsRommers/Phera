//! Give modern documents, fragments and elements their shared PHP interfaces.
//!
//! ```php
//! function find_links(Dom\ParentNode $root): Dom\NodeList {
//!     return $root->querySelectorAll('a[href]');
//! }
//! ```

use phera_extension_api::{ClassEntry, MethodEntry as M, Param as P, ParamType as T, Registry};

use super::super::{document, mutation};
use super::selectors;

const NODES: P = document::VARIADIC;
const SELECTORS: P = P::new("selectors", T::String);

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        methods: const {
            &[
                M::required("append", &[NODES]).returning("void"),
                M::required("prepend", &[NODES]).returning("void"),
                M::required("replaceChildren", &[NODES]).returning("void"),
                M::required("querySelector", &[SELECTORS]).returning("?Dom\\Element"),
                M::required("querySelectorAll", &[SELECTORS]).returning("Dom\\NodeList"),
            ]
        },
        ..ClassEntry::interface("Dom\\ParentNode")
    });
    registry.class(ClassEntry {
        methods: const {
            &[
                M::required("remove", &[]).returning("void"),
                M::required("before", &[NODES]).returning("void"),
                M::required("after", &[NODES]).returning("void"),
                M::required("replaceWith", &[NODES]).returning("void"),
            ]
        },
        ..ClassEntry::interface("Dom\\ChildNode")
    });
    registry.class(ClassEntry {
        parent: Some("Dom\\Node"),
        interfaces: &["Dom\\ParentNode"],
        methods: const {
            &[
                M::public("append", &[NODES], mutation::append).returning("void"),
                M::public("prepend", &[NODES], mutation::prepend).returning("void"),
                M::public("replaceChildren", &[NODES], mutation::replace_children)
                    .returning("void"),
                M::public("querySelector", &[SELECTORS], selectors::first)
                    .returning("?Dom\\Element"),
                M::public("querySelectorAll", &[SELECTORS], selectors::all)
                    .returning("Dom\\NodeList"),
            ]
        },
        ..ClassEntry::class("Dom\\DocumentFragment")
    });
}
