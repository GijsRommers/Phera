//! `pack('nC', 258, 3)` writes the bytes `01 02 03`.

mod format;
mod pack;
mod unpack;

pub use pack::PACK;
pub use unpack::UNPACK;
