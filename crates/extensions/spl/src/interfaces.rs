//! Iterator and observer protocols used by SPL and implemented by PHP classes.
//!
//! ```php
//! function jump(SeekableIterator $rows): void { $rows->seek(3); }
//! ```

use phera_extension_api::{ClassEntry, MethodEntry, Param, ParamType, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Iterator"],
        methods: const {
            &[
                MethodEntry::required("seek", const { &[Param::new("offset", ParamType::Int)] })
                    .tentative_return("void"),
            ]
        },
        ..ClassEntry::interface("SeekableIterator")
    });
    registry.class(ClassEntry {
        interfaces: &["Iterator"],
        methods: const {
            &[
                MethodEntry::required("hasChildren", &[]).tentative_return("bool"),
                MethodEntry::required("getChildren", &[]).tentative_return("?RecursiveIterator"),
            ]
        },
        ..ClassEntry::interface("RecursiveIterator")
    });
    registry.class(ClassEntry {
        interfaces: &["Iterator"],
        methods: const {
            &[MethodEntry::required("getInnerIterator", &[]).tentative_return("?Iterator")]
        },
        ..ClassEntry::interface("OuterIterator")
    });
    registry.class(ClassEntry {
        methods: const {
            &[MethodEntry::required(
                "update",
                const { &[Param::new("subject", ParamType::Class("SplSubject"))] },
            )
            .tentative_return("void")]
        },
        ..ClassEntry::interface("SplObserver")
    });
    const OBSERVER: &[Param] = const { &[Param::new("observer", ParamType::Class("SplObserver"))] };
    registry.class(ClassEntry {
        methods: const {
            &[
                MethodEntry::required("attach", OBSERVER).tentative_return("void"),
                MethodEntry::required("detach", OBSERVER).tentative_return("void"),
                MethodEntry::required("notify", &[]).tentative_return("void"),
            ]
        },
        ..ClassEntry::interface("SplSubject")
    });
}
