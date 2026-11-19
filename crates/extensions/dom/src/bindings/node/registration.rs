//! PHP signatures for the base node methods and DOM interfaces.
//!
//! ```php
//! $parent->insertBefore(node: $child, child: $sibling);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue as C, MethodEntry as M, NativeClone, Param as P,
    ParamType as T, Registry,
};

use super::super::{characters, mutation, properties};
use super::*;

pub(in crate::bindings) fn register(registry: &mut Registry) {
    use super::super::document::VARIADIC;
    registry.class(ClassEntry {
        methods: const {
            &[
                M::required("append", &[VARIADIC]).returning("void"),
                M::required("prepend", &[VARIADIC]).returning("void"),
                M::required("replaceChildren", &[VARIADIC]).returning("void"),
            ]
        },
        ..ClassEntry::interface("DOMParentNode")
    });
    registry.class(ClassEntry {
        methods: const {
            &[
                M::required("before", &[VARIADIC]).returning("void"),
                M::required("after", &[VARIADIC]).returning("void"),
                M::required("replaceWith", &[VARIADIC]).returning("void"),
                M::required("remove", &[]).returning("void"),
            ]
        },
        ..ClassEntry::interface("DOMChildNode")
    });
    const NODE: T = T::Class("DOMNode");
    const FEATURE: &[P] = const { &[P::new("feature", T::String), P::new("version", T::String)] };
    registry.class(ClassEntry {
        hooks: Some(&ClassHooks {
            clone: Some(NativeClone::CopyWithHandle(clone_object)),
            property_access: Some(&properties::ACCESS),
            properties: Some(properties::view::view),
            ..ClassHooks::EMPTY
        }),
        constants: &[
            ("DOCUMENT_POSITION_DISCONNECTED", C::Int(1)),
            ("DOCUMENT_POSITION_PRECEDING", C::Int(2)),
            ("DOCUMENT_POSITION_FOLLOWING", C::Int(4)),
            ("DOCUMENT_POSITION_CONTAINS", C::Int(8)),
            ("DOCUMENT_POSITION_CONTAINED_BY", C::Int(16)),
            ("DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC", C::Int(32)),
        ],
        methods: const {
            const {
                &[
                    M::public(
                        "appendChild",
                        const { &[P::new("node", NODE)] },
                        mutation::append_child,
                    ),
                    M::public(
                        "removeChild",
                        const { &[P::new("child", NODE)] },
                        mutation::remove_child,
                    ),
                    M::public(
                        "insertBefore",
                        const {
                            &[
                                P::new("node", NODE),
                                P::new("child", NODE).nullable().with_default(C::Null),
                            ]
                        },
                        mutation::insert_before,
                    ),
                    M::public(
                        "replaceChild",
                        const { &[P::new("node", NODE), P::new("child", NODE)] },
                        mutation::replace_child,
                    ),
                    M::public("hasChildNodes", &[], has_children).tentative_return("bool"),
                    M::public("hasAttributes", &[], has_attributes).tentative_return("bool"),
                    M::public("getLineNo", &[], line).tentative_return("int"),
                    M::public("getNodePath", &[], node_path).tentative_return("?string"),
                    M::public(
                        "getRootNode",
                        const { &[P::new("options", T::Array).nullable().with_default(C::Null)] },
                        root,
                    )
                    .returning("DOMNode"),
                    M::public(
                        "cloneNode",
                        const { &[P::new("deep", T::Bool).with_default(C::Bool(false))] },
                        clone_node,
                    ),
                    M::public(
                        "contains",
                        const { &[P::new("other", T::Mixed)] },
                        compare::contains,
                    )
                    .returning("bool"),
                    M::public(
                        "isSameNode",
                        const { &[P::new("otherNode", NODE)] },
                        compare::same,
                    )
                    .tentative_return("bool"),
                    M::public(
                        "isEqualNode",
                        const { &[P::new("otherNode", NODE).nullable()] },
                        compare::equal,
                    )
                    .returning("bool"),
                    M::public(
                        "compareDocumentPosition",
                        const { &[P::new("other", NODE)] },
                        compare::position,
                    )
                    .returning("int"),
                    M::public("normalize", &[], normalize).tentative_return("void"),
                    M::public("isSupported", FEATURE, factory::has_feature)
                        .tentative_return("bool"),
                    M::public(
                        "isDefaultNamespace",
                        const { &[P::new("namespace", T::String)] },
                        default_namespace,
                    )
                    .tentative_return("bool"),
                    M::public(
                        "lookupNamespaceURI",
                        const { &[P::new("prefix", T::String).nullable()] },
                        lookup_namespace,
                    )
                    .tentative_return("?string"),
                    M::public(
                        "lookupPrefix",
                        const { &[P::new("namespace", T::String)] },
                        lookup_prefix,
                    )
                    .tentative_return("?string"),
                    M::public(
                        "C14N",
                        const {
                            &[
                                P::new("exclusive", T::Bool).with_default(C::Bool(false)),
                                P::new("withComments", T::Bool).with_default(C::Bool(false)),
                                P::new("xpath", T::Array).nullable().with_default(C::Null),
                                P::new("nsPrefixes", T::Array)
                                    .nullable()
                                    .with_default(C::Null),
                            ]
                        },
                        canonical::c14n,
                    )
                    .tentative_return("string|false"),
                    M::public_async(
                        "C14NFile",
                        const {
                            &[
                                P::new("uri", T::String),
                                P::new("exclusive", T::Bool).with_default(C::Bool(false)),
                                P::new("withComments", T::Bool).with_default(C::Bool(false)),
                                P::new("xpath", T::Array).nullable().with_default(C::Null),
                                P::new("nsPrefixes", T::Array)
                                    .nullable()
                                    .with_default(C::Null),
                            ]
                        },
                        canonical::c14n_file,
                    )
                    .tentative_return("int|false"),
                    M::public("__sleep", &[], serialization_forbidden).returning("array"),
                    M::public("__wakeup", &[], serialization_forbidden).returning("void"),
                ]
            }
        },
        ..ClassEntry::class("DOMNode")
    });
    characters::register(registry);
    super::super::attributes::register(registry);
    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        ..ClassEntry::class("DOMDocumentType")
    });
    for class in ["DOMEntity", "DOMNotation"] {
        registry.class(ClassEntry {
            parent: Some("DOMNode"),
            ..ClassEntry::class(class)
        });
    }
}
