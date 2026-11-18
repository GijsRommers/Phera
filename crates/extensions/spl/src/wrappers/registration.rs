//! SPL iterator adapters share IteratorIterator's protected inner traversal.
//!
//! ```php
//! class Parents extends RecursiveFilterIterator {
//!     public function accept(): bool { return $this->hasChildren(); }
//! }
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, MethodEntry as Method, NativeClone, Param,
    ParamType, Registry,
};

use super::{filter, limit, methods as m};

const ITERATOR: Param = Param::new("iterator", ParamType::Class("Iterator"));
const RECURSIVE: Param = Param::new("iterator", ParamType::Class("RecursiveIterator"));
const CALLBACK: Param = Param::new("callback", ParamType::Callable);
const TRAVERSABLE: Param = Param::new("iterator", ParamType::Class("Traversable"));
const OPTIONAL_CLASS: Param = Param::new("class", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);
const OFFSET: Param = Param::new("offset", ParamType::Int);
const LENGTH: Param = Param::new("limit", ParamType::Int).with_default(ConstantValue::Int(-1));

const NOT_CLONEABLE: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    forward_method: Some(super::forward_method),
    ..ClassHooks::EMPTY
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["Iterator"],
        methods: const {
            &[
                Method::public("current", &[], m::empty_current).tentative_return("never"),
                Method::public("next", &[], m::no_action).tentative_return("void"),
                Method::public("key", &[], m::empty_key).tentative_return("never"),
                Method::public("valid", &[], m::empty_valid).tentative_return("false"),
                Method::public("rewind", &[], m::no_action).tentative_return("void"),
            ]
        },
        ..ClassEntry::class("EmptyIterator")
    });
    registry.class(ClassEntry {
        interfaces: &["OuterIterator"],
        hooks: Some(&NOT_CLONEABLE),
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    &[TRAVERSABLE, OPTIONAL_CLASS],
                    m::construct_base,
                ),
                Method::public("getInnerIterator", &[], m::get_inner).tentative_return("?Iterator"),
                Method::public_async("rewind", &[], m::on_rewind).tentative_return("void"),
                Method::public("valid", &[], m::valid).tentative_return("bool"),
                Method::public("key", &[], m::key).tentative_return("mixed"),
                Method::public("current", &[], m::current).tentative_return("mixed"),
                Method::public_async("next", &[], m::on_next).tentative_return("void"),
            ]
        },
        ..ClassEntry::class("IteratorIterator")
    });
    registry.class(ClassEntry {
        parent: Some("IteratorIterator"),
        kind: ClassKind::AbstractClass,
        methods: const {
            &[
                Method::public_async("__construct", &[ITERATOR], m::construct),
                Method::required("accept", &[]).tentative_return("bool"),
                Method::public_async("rewind", &[], filter::on_rewind).tentative_return("void"),
                Method::public_async("next", &[], filter::on_next).tentative_return("void"),
            ]
        },
        ..ClassEntry::class("FilterIterator")
    });
    registry.class(ClassEntry {
        parent: Some("FilterIterator"),
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    &[ITERATOR, CALLBACK],
                    filter::construct_callback,
                ),
                Method::public_async("accept", &[], filter::accept).tentative_return("bool"),
            ]
        },
        ..ClassEntry::class("CallbackFilterIterator")
    });
    registry.class(ClassEntry {
        parent: Some("CallbackFilterIterator"),
        interfaces: &["RecursiveIterator"],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    &[RECURSIVE, CALLBACK],
                    filter::construct_callback,
                ),
                Method::public_async("hasChildren", &[], filter::has_children)
                    .tentative_return("bool"),
                Method::public_async("getChildren", &[], filter::children)
                    .tentative_return("RecursiveCallbackFilterIterator"),
            ]
        },
        ..ClassEntry::class("RecursiveCallbackFilterIterator")
    });
    registry.class(ClassEntry {
        parent: Some("FilterIterator"),
        kind: ClassKind::AbstractClass,
        interfaces: &["RecursiveIterator"],
        methods: const {
            &[
                Method::public_async("__construct", &[RECURSIVE], m::construct),
                Method::public_async("hasChildren", &[], filter::has_children)
                    .tentative_return("bool"),
                Method::public_async("getChildren", &[], filter::children)
                    .tentative_return("?RecursiveFilterIterator"),
            ]
        },
        ..ClassEntry::class("RecursiveFilterIterator")
    });
    registry.class(ClassEntry {
        parent: Some("RecursiveFilterIterator"),
        methods: const {
            &[
                Method::public_async("__construct", &[RECURSIVE], m::construct),
                Method::public_async("accept", &[], filter::has_children).tentative_return("bool"),
            ]
        },
        ..ClassEntry::class("ParentIterator")
    });
    registry.class(ClassEntry {
        parent: Some("IteratorIterator"),
        methods: const {
            &[
                Method::public_async("__construct", &[ITERATOR], m::construct),
                Method::public_async("next", &[], m::infinite_next).tentative_return("void"),
            ]
        },
        ..ClassEntry::class("InfiniteIterator")
    });
    registry.class(ClassEntry {
        parent: Some("IteratorIterator"),
        methods: const {
            &[
                Method::public_async("__construct", &[ITERATOR], m::construct),
                Method::public("rewind", &[], m::no_action).tentative_return("void"),
                Method::public_async("valid", &[], m::no_rewind_valid).tentative_return("bool"),
                Method::public_async("key", &[], m::no_rewind_key).tentative_return("mixed"),
                Method::public_async("current", &[], m::no_rewind_current)
                    .tentative_return("mixed"),
                Method::public_async("next", &[], m::no_rewind_next).tentative_return("void"),
            ]
        },
        ..ClassEntry::class("NoRewindIterator")
    });
    registry.class(ClassEntry {
        parent: Some("IteratorIterator"),
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &[ITERATOR, OFFSET.with_default(ConstantValue::Int(0)), LENGTH] },
                    limit::construct,
                ),
                Method::public_async("rewind", &[], limit::on_rewind).tentative_return("void"),
                Method::public("valid", &[], m::valid).tentative_return("bool"),
                Method::public_async("next", &[], limit::on_next).tentative_return("void"),
                Method::public_async("seek", &[OFFSET], limit::seek).tentative_return("int"),
                Method::public("getPosition", &[], limit::position).tentative_return("int"),
            ]
        },
        ..ClassEntry::class("LimitIterator")
    });
    super::append::register(registry);
    super::cache::register(registry);
    super::regex::register(registry);
}
