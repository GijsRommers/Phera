//! Builtin signatures generated from PHP stubs, e.g. `strlen(string $string): int`.
//! The table is committed; compiling and checking never require PHP or php-src.

mod decode;
mod encode;
mod fields;

use crate::index::Index;
pub use encode::encode;
use phera_source::deprecations::{ItemKind, Policy, items};

pub fn load() -> Index {
    let mut index = decode::decode(include_str!("../data/php85.tsv"));
    index.builtin_functions = index.functions.keys().cloned().collect();
    index.builtin_classes = index.classes.keys().cloned().collect();
    index
}

pub(crate) fn load_supported() -> Index {
    let mut index = load();
    for item in items() {
        if item.since.is_empty()
            && let Some((class, _)) = item.name.split_once("::")
        {
            // phpredis methods are external to the PHP stub inventory.
            index.builtin_classes.insert(class.to_ascii_lowercase());
        }
        if item.policy == Policy::Keep {
            continue;
        }
        match item.kind() {
            ItemKind::Function => {
                let name = item.name.to_ascii_lowercase();
                index.functions.remove(&name);
                index.builtin_functions.remove(&name);
            }
            ItemKind::Constant => {
                index.constants.remove(item.name);
            }
            ItemKind::Method | ItemKind::ClassConstant => {
                if let Some((class, member)) = item.name.split_once("::")
                    && let Some(class) = index.classes.get_mut(&class.to_ascii_lowercase())
                {
                    if item.kind() == ItemKind::Method {
                        class.methods.remove(&member.to_ascii_lowercase());
                    } else {
                        class.constants.remove(member);
                    }
                }
            }
        }
    }
    index
}
