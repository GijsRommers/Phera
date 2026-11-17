//! The hash extension compared with real PHP 8.5, plus published test
//! vectors. One test binary, so the modules share [`support`] and run in
//! parallel.

mod algorithms;
mod compositions;
mod files;
mod parameters;
mod password_validation;
mod passwords;
mod support;
mod vectors;
mod xxhash_secrets;
