//! Builtin signatures generated from PHP stubs, e.g. `strlen(string $string): int`.
//! The table is committed; compiling and checking never require PHP or php-src.

mod decode;
mod encode;
mod fields;

use crate::index::Index;
pub use encode::encode;

pub fn load() -> Index {
    let mut index = decode::decode(include_str!("../data/php85.tsv"));
    index.builtin_functions = index.functions.keys().cloned().collect();
    index
}
