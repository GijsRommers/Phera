//! HTTP query strings and multipart fields share PHP's form control name rules.

pub(crate) use phera_ext_standard::query::{Duplicates, parse_pairs, parse_post, store_named};
