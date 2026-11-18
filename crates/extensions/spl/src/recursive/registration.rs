//! Recursive iteration exposes traversal order, depth controls and tree formatting.
//!
//! ```php
//! $it = new RecursiveIteratorIterator($nodes, RecursiveIteratorIterator::SELF_FIRST);
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, MethodEntry as Method, NativeClone, Param, ParamType,
    Registry,
};

use super::{CLASS, TREE, methods as m, tree};

const ITERATOR: Param = Param::new("iterator", ParamType::Class("Traversable"));
const TREE_ITERATOR: Param = Param::new("iterator", ParamType::RecursiveOrAggregate);
const MODE: Param = Param::new("mode", ParamType::Int);
const FLAGS: Param = Param::new("flags", ParamType::Int);
const CACHE_FLAGS: Param = Param::new("cachingIteratorFlags", ParamType::Int);
const PART: Param = Param::new("part", ParamType::Int);
const VALUE: Param = Param::new("value", ParamType::String);
const POSTFIX: Param = Param::new("postfix", ParamType::String);
const LEVEL: Param = Param::new("level", ParamType::Int)
    .nullable()
    .with_default(ConstantValue::Null);
const MAX_DEPTH: Param =
    Param::new("maxDepth", ParamType::Int).with_default(ConstantValue::Int(-1));

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    ..ClassHooks::EMPTY
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["OuterIterator"],
        hooks: Some(&HOOKS),
        constants: &[
            ("LEAVES_ONLY", ConstantValue::Int(0)),
            ("SELF_FIRST", ConstantValue::Int(1)),
            ("CHILD_FIRST", ConstantValue::Int(2)),
            ("CATCH_GET_CHILD", ConstantValue::Int(16)),
        ],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const {
                        &[
                            ITERATOR,
                            MODE.with_default(ConstantValue::Int(0)),
                            FLAGS.with_default(ConstantValue::Int(0)),
                        ]
                    },
                    m::construct,
                ),
                Method::public_async("rewind", &[], m::rewind).tentative_return("void"),
                Method::public_async("valid", &[], m::valid).tentative_return("bool"),
                Method::public_async("key", &[], m::key).tentative_return("mixed"),
                Method::public_async("current", &[], m::current).tentative_return("mixed"),
                Method::public_async("next", &[], m::next).tentative_return("void"),
                Method::public("getDepth", &[], m::get_depth).tentative_return("int"),
                Method::public("getSubIterator", &[LEVEL], m::get_sub)
                    .tentative_return("?RecursiveIterator"),
                Method::public("getInnerIterator", &[], m::get_inner)
                    .tentative_return("RecursiveIterator"),
                Method::public("beginIteration", &[], m::no_action).tentative_return("void"),
                Method::public("endIteration", &[], m::no_action).tentative_return("void"),
                Method::public_async("callHasChildren", &[], m::has_children)
                    .tentative_return("bool"),
                Method::public_async("callGetChildren", &[], m::children)
                    .tentative_return("?RecursiveIterator"),
                Method::public("beginChildren", &[], m::no_action).tentative_return("void"),
                Method::public("endChildren", &[], m::no_action).tentative_return("void"),
                Method::public("nextElement", &[], m::no_action).tentative_return("void"),
                Method::public("setMaxDepth", &[MAX_DEPTH], m::set_max).tentative_return("void"),
                Method::public("getMaxDepth", &[], m::get_max).tentative_return("int|false"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        constants: &[
            ("BYPASS_CURRENT", ConstantValue::Int(4)),
            ("BYPASS_KEY", ConstantValue::Int(8)),
            ("PREFIX_LEFT", ConstantValue::Int(0)),
            ("PREFIX_MID_HAS_NEXT", ConstantValue::Int(1)),
            ("PREFIX_MID_LAST", ConstantValue::Int(2)),
            ("PREFIX_END_HAS_NEXT", ConstantValue::Int(3)),
            ("PREFIX_END_LAST", ConstantValue::Int(4)),
            ("PREFIX_RIGHT", ConstantValue::Int(5)),
        ],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const {
                        &[
                            TREE_ITERATOR,
                            FLAGS.with_default(ConstantValue::Int(8)),
                            CACHE_FLAGS.with_default(ConstantValue::Int(16)),
                            MODE.with_default(ConstantValue::Int(1)),
                        ]
                    },
                    tree::construct,
                ),
                Method::public_async("key", &[], tree::key).tentative_return("mixed"),
                Method::public_async("current", &[], tree::current).tentative_return("mixed"),
                Method::public_async("getPrefix", &[], tree::get_prefix).tentative_return("string"),
                Method::public("setPostfix", &[POSTFIX], tree::set_postfix)
                    .tentative_return("void"),
                Method::public("setPrefixPart", &[PART, VALUE], tree::set_part)
                    .tentative_return("void"),
                Method::public_async("getEntry", &[], tree::get_entry).tentative_return("string"),
                Method::public("getPostfix", &[], tree::get_postfix).tentative_return("string"),
            ]
        },
        ..ClassEntry::class(TREE)
    });
}
