//! Register the modern XPath receiver and its stronger PHP return types.
//!
//! ```php
//! $nodes = (new Dom\XPath($document))->query('//item');
//! ```

use phera_extension_api::ClassKind;

use super::*;

pub(in crate::bindings) fn register_modern(registry: &mut Registry) {
    const ARGS: &[P] = &[
        P::new("expression", T::String),
        P::new("contextNode", T::Class("Dom\\Node"))
            .nullable()
            .with_default(C::Null),
        P::new("registerNodeNS", T::Bool).with_default(C::Bool(true)),
    ];
    let hooks = registry.get_class("DOMXPath").and_then(|class| class.hooks);
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks,
        methods: const {
            &[
                M::public(
                    "__construct",
                    const {
                        &[
                            P::new("document", T::Class("Dom\\Document")),
                            P::new("registerNodeNS", T::Bool).with_default(C::Bool(true)),
                        ]
                    },
                    construct,
                ),
                M::public_async("query", ARGS, query).returning("Dom\\NodeList"),
                M::public_async("evaluate", ARGS, expression)
                    .returning("null|bool|float|string|Dom\\NodeList"),
                M::public(
                    "registerNamespace",
                    const { &[P::new("prefix", T::String), P::new("namespace", T::String)] },
                    namespace,
                )
                .returning("bool"),
                M::public(
                    "registerPhpFunctions",
                    const {
                        &[P::new("restrict", T::ArrayOrString)
                            .nullable()
                            .with_default(C::Null)]
                    },
                    php::register,
                )
                .returning("void"),
                M::public(
                    "registerPhpFunctionNS",
                    const {
                        &[
                            P::new("namespaceURI", T::String),
                            P::new("name", T::String),
                            P::new("callable", T::Callable),
                        ]
                    },
                    php::register_ns,
                )
                .returning("void"),
                M::public("quote", const { &[P::new("str", T::String)] }, quote)
                    .static_()
                    .returning("string"),
            ]
        },
        ..ClassEntry::class("Dom\\XPath")
    });
}
