//! PHP JSON option bits, decoded once for each operation.
pub const HEX_TAG: i64 = 1;
pub const HEX_AMP: i64 = 2;
pub const HEX_APOS: i64 = 4;
pub const HEX_QUOT: i64 = 8;
pub const UNESCAPED_SLASHES: i64 = 64;
pub const UNESCAPED_UNICODE: i64 = 256;
pub const PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
pub const PRESERVE_ZERO_FRACTION: i64 = 1024;
pub const UNESCAPED_LINE_TERMINATORS: i64 = 2048;
pub const INVALID_UTF8_IGNORE: i64 = 1 << 20;
pub const INVALID_UTF8_SUBSTITUTE: i64 = 1 << 21;
pub const THROW_ON_ERROR: i64 = 1 << 22;

/// Named options keep encoder and parser branches auditable.
#[derive(Clone, Copy, Default)]
pub(crate) struct Flags {
    pub hex_tag: bool,
    pub hex_amp: bool,
    pub hex_apos: bool,
    pub hex_quot: bool,
    pub force_object: bool,
    pub numeric: bool,
    pub slashes: bool,
    pub pretty: bool,
    pub unicode: bool,
    pub partial: bool,
    pub fraction: bool,
    pub lines: bool,
    pub ignore: bool,
    pub substitute: bool,
    pub throw: bool,
    pub associative: bool,
    pub bigint: bool,
}
impl Flags {
    /// PHP ignores unknown bits except in json_validate.
    pub fn new(bits: i64) -> Self {
        Self {
            hex_tag: bits & 1 != 0,
            hex_amp: bits & 2 != 0,
            hex_apos: bits & 4 != 0,
            hex_quot: bits & 8 != 0,
            force_object: bits & 16 != 0,
            numeric: bits & 32 != 0,
            slashes: bits & 64 != 0,
            pretty: bits & 128 != 0,
            unicode: bits & 256 != 0,
            partial: bits & 512 != 0,
            fraction: bits & 1024 != 0,
            lines: bits & 2048 != 0,
            ignore: bits & 1048576 != 0,
            substitute: bits & 2097152 != 0,
            throw: bits & 4194304 != 0,
            associative: bits & 1 != 0,
            bigint: bits & 2 != 0,
        }
    }
}
