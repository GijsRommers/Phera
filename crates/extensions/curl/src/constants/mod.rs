//! The PHP 8.5 cURL constant catalog, including aliases.
//!
//! `echo CURLOPT_URL;` prints the numeric option identifier.

use phera_extension_api::Registry;

mod errors_1;
pub(crate) mod ids;
mod info_1;
mod options_1;
mod options_2;
mod other_1;
mod other_2;

const TABLES: &[&[(&str, i64)]] = &[
    options_1::VALUES,
    options_2::VALUES,
    info_1::VALUES,
    errors_1::VALUES,
    other_1::VALUES,
    other_2::VALUES,
];

pub(crate) fn register(registry: &mut Registry) {
    for &(name, value) in TABLES.iter().flat_map(|table| table.iter()) {
        registry.constant(name, value);
    }
}

pub(crate) fn option_name(value: i64) -> Option<&'static str> {
    options_1::VALUES
        .iter()
        .chain(options_2::VALUES)
        .find_map(|&(name, number)| (number == value).then_some(name))
}
