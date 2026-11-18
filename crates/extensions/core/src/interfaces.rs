//! Engine interfaces required by language operations.
//!
//! ```php
//! class Bag implements Countable {
//!     public function count(): int { return 3; }
//! }
//! count(new Bag); // 3
//! ```

use phera_extension_api::{ClassEntry, MethodEntry, Param, ParamType, Registry};

const OFFSET: Param = Param::new("offset", ParamType::Mixed);
const SET_PARAMS: &[Param] = &[OFFSET, Param::new("value", ParamType::Mixed)];

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry::interface("Traversable"));
    registry.class(ClassEntry {
        interfaces: &["Traversable"],
        methods: const {
            &[
                MethodEntry::required("current", &[]).tentative_return("mixed"),
                MethodEntry::required("next", &[]).tentative_return("void"),
                MethodEntry::required("key", &[]).tentative_return("mixed"),
                MethodEntry::required("valid", &[]).tentative_return("bool"),
                MethodEntry::required("rewind", &[]).tentative_return("void"),
            ]
        },
        ..ClassEntry::interface("Iterator")
    });
    registry.class(ClassEntry {
        interfaces: &["Traversable"],
        methods: const { &[MethodEntry::required("getIterator", &[]).tentative_return("Traversable")] },
        ..ClassEntry::interface("IteratorAggregate")
    });
    registry.class(ClassEntry {
        methods: const {
            &[
                MethodEntry::required("offsetExists", &[OFFSET]).tentative_return("bool"),
                MethodEntry::required("offsetGet", &[OFFSET]).tentative_return("mixed"),
                MethodEntry::required("offsetSet", SET_PARAMS).tentative_return("void"),
                MethodEntry::required("offsetUnset", &[OFFSET]).tentative_return("void"),
            ]
        },
        ..ClassEntry::interface("ArrayAccess")
    });
    registry.class(ClassEntry {
        methods: const { &[MethodEntry::required("count", &[]).tentative_return("int")] },
        ..ClassEntry::interface("Countable")
    });
}
