//! The legacy SPL methods put PHP wire-format values between class-specific separators.
//!
//! ```php
//! (new ArrayObject([7]))->serialize(); // x:i:0;a:1:{i:0;i:7;};m:a:0:{}
//! ```

mod read;
mod write;

pub(crate) use read::Reader;
pub(crate) use write::Writer;
