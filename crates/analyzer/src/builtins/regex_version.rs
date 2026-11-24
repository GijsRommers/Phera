//! `PCRE_VERSION_MAJOR < 10` is false on Phera's bundled PCRE runtime.

use crate::{index::Index, types::ValueType};

pub(crate) fn add_constants(index: &mut Index) {
    // Match the constants registered by crates/extensions/pcre.
    for (name, value) in [("PCRE_VERSION_MAJOR", 10), ("PCRE_VERSION_MINOR", 46)] {
        if let Some(constant) = index.constants.get_mut(name) {
            constant.ty = ValueType::IntLiteral(value);
        }
    }
}
