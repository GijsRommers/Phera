//! Regex adapters support PHP's five matching modes and separate PCRE flags.
//!
//! ```php
//! new RecursiveRegexIterator($tree, '/item/', RecursiveRegexIterator::MATCH);
//! ```

use phera_extension_api::{
    ClassEntry, ConstantValue, MethodEntry as Method, Param, ParamType, PropertyEntry,
    PropertyType, Registry, Visibility,
};

use super::{CLASS, RECURSIVE, matching};

const ITERATOR: Param = Param::new("iterator", ParamType::Class("Iterator"));
const CHILD: Param = Param::new("iterator", ParamType::Class("RecursiveIterator"));
const PATTERN: Param = Param::new("pattern", ParamType::String);
const MODE: Param = Param::new("mode", ParamType::Int);
const FLAGS: Param = Param::new("flags", ParamType::Int);
const PREG_FLAGS: Param = Param::new("pregFlags", ParamType::Int);
const fn constructor(iterator: Param) -> [Param; 5] {
    [
        iterator,
        PATTERN,
        MODE.with_default(ConstantValue::Int(0)),
        FLAGS.with_default(ConstantValue::Int(0)),
        PREG_FLAGS.with_default(ConstantValue::Int(0)),
    ]
}

pub(in crate::wrappers) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("FilterIterator"),
        constants: &[
            ("USE_KEY", ConstantValue::Int(1)),
            ("INVERT_MATCH", ConstantValue::Int(2)),
            ("MATCH", ConstantValue::Int(0)),
            ("GET_MATCH", ConstantValue::Int(1)),
            ("ALL_MATCHES", ConstantValue::Int(2)),
            ("SPLIT", ConstantValue::Int(3)),
            ("REPLACE", ConstantValue::Int(4)),
        ],
        properties: const {
            &[PropertyEntry::typed(
                "replacement",
                PropertyType::NullableString,
                Visibility::Public,
            )
            .with_default(ConstantValue::Null)]
        },
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &constructor(ITERATOR) },
                    super::construct,
                ),
                Method::public_async("accept", &[], matching::accept).tentative_return("bool"),
                Method::public("getMode", &[], super::get_mode).tentative_return("int"),
                Method::public("setMode", &[MODE], super::set_mode).tentative_return("void"),
                Method::public("getFlags", &[], super::get_flags).tentative_return("int"),
                Method::public("setFlags", &[FLAGS], super::set_flags).tentative_return("void"),
                Method::public("getRegex", &[], super::get_regex).tentative_return("string"),
                Method::public("getPregFlags", &[], super::get_preg_flags).tentative_return("int"),
                Method::public("setPregFlags", &[PREG_FLAGS], super::set_preg_flags)
                    .tentative_return("void"),
            ]
        },
        ..ClassEntry::class(CLASS)
    });
    registry.class(ClassEntry {
        parent: Some(CLASS),
        interfaces: &["RecursiveIterator"],
        methods: const {
            &[
                Method::public_async(
                    "__construct",
                    const { &constructor(CHILD) },
                    super::construct,
                ),
                Method::public_async("accept", &[], matching::accept).tentative_return("bool"),
                Method::public_async("hasChildren", &[], crate::wrappers::filter::has_children)
                    .tentative_return("bool"),
                Method::public_async("getChildren", &[], matching::children)
                    .tentative_return(RECURSIVE),
            ]
        },
        ..ClassEntry::class(RECURSIVE)
    });
}
