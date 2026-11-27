//! On-disk `.phb` binary layout constants, header, section directory, and ABI hash.

use phera_bytecode::facts::{BuildMode, DynamicKind};

use crate::opcodes;

/// Eight-byte magic header (`\x89PHB\r\n\x1a\n`), patterned after PNG to catch
/// newline-translation corruption immediately.
pub const MAGIC: [u8; 8] = *b"\x89PHB\r\n\x1a\n";

/// Wire format version for `.phb` files.
pub const FORMAT_VERSION: u16 = 1;

/// Monotonic revision bumped whenever non-opcode bytecode semantics change.
pub const BYTECODE_REVISION: u32 = 1;

/// Fixed header size in bytes.
pub const HEADER_LEN: u32 = 64;

/// Fixed section directory entry size in bytes.
pub const SECTION_ENTRY_LEN: usize = 32;

/// Fixed unit directory entry size in bytes inside the `UNITS` section.
pub const UNIT_ENTRY_LEN: usize = 24;

/// Header flag bitset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageFlags(pub u16);

impl ImageFlags {
    pub const NONE: Self = Self(0);
    pub const STRICT: Self = Self(1 << 0);
    pub const HAS_FACTS: Self = Self(1 << 1);
    pub const WEB_UNITS: Self = Self(1 << 2);
    pub const CLI_UNITS: Self = Self(1 << 3);
    pub const ALLOW_EVAL: Self = Self(1 << 4);
    pub const ALLOW_CLASS_ALIAS: Self = Self(1 << 5);
    pub const KNOWN_MASK: Self = Self(
        Self::STRICT.0
            | Self::HAS_FACTS.0
            | Self::WEB_UNITS.0
            | Self::CLI_UNITS.0
            | Self::ALLOW_EVAL.0
            | Self::ALLOW_CLASS_ALIAS.0,
    );

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Section identifiers in the `.phb` section directory.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Manifest = 1,
    Strings = 2,
    Files = 3,
    Classmap = 4,
    Units = 5,
    Facts = 6,
    Audit = 7,
}

impl SectionKind {
    pub fn from_u32(raw: u32) -> Option<Self> {
        match raw {
            1 => Some(Self::Manifest),
            2 => Some(Self::Strings),
            3 => Some(Self::Files),
            4 => Some(Self::Classmap),
            5 => Some(Self::Units),
            6 => Some(Self::Facts),
            7 => Some(Self::Audit),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Manifest => "MANIFEST",
            Self::Strings => "STRINGS",
            Self::Files => "FILES",
            Self::Classmap => "CLASSMAP",
            Self::Units => "UNITS",
            Self::Facts => "FACTS",
            Self::Audit => "AUDIT",
        }
    }
}

/// Parsed 64-byte `.phb` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHeader {
    pub format_version: u16,
    pub flags: ImageFlags,
    pub header_len: u32,
    pub runtime_abi_hash: [u8; 16],
    pub body_checksum: [u8; 16],
    pub file_len: u64,
    pub section_count: u32,
}

/// Parsed 32-byte directory entry for one section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionEntry {
    pub kind: SectionKind,
    pub flags: u32,
    pub offset: u64,
    pub len: u64,
    pub check: u64,
}

/// File-entry flags in the `FILES` section.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileFlags(pub u32);

impl FileFlags {
    pub const NONE: Self = Self(0);
    pub const ALWAYS_LOADED: Self = Self(1 << 0);
    pub const ENTRY: Self = Self(1 << 1);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Which runtime target a unit in the image was compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageTarget {
    Web,
    Cli,
}

/// Decoded `MANIFEST` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageManifest {
    pub project_root: String,
    pub mode: BuildMode,
    pub has_web: bool,
    pub has_cli: bool,
    pub allow: Vec<DynamicKind>,
    pub framework: Option<String>,
    pub entries: Vec<String>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub include_dev: bool,
}

/// One file recorded in the `FILES` section (sorted by `relative_path`).
///
/// Note: `phera_bytecode::facts::UnitRef` indexes into this `FILES` table
/// (`0..files.len()`), not into the `UNITS` blob directory (since a single file
/// may store separate Web and CLI unit blobs when built with `--target both`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFileEntry {
    pub relative_path: String,
    pub canonical_path: String,
    pub unit_web: Option<u32>,
    pub unit_cli: Option<u32>,
    pub source_len: u64,
    pub source_hash: [u8; 16],
    pub flags: FileFlags,
}

/// One class entry recorded in the `CLASSMAP` section (sorted by lowercased name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageClassEntry {
    pub name: String,
    pub file_index: u32,
    pub class_id: u32,
    pub class_ref: Option<u32>,
}

/// Directory entry for one unit blob inside the `UNITS` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitDirEntry {
    pub rel_offset: u64,
    pub len: u32,
    pub check: u64,
}

/// Computes a 16-byte BLAKE2b-128 digest.
pub fn blake2b_128(bytes: &[u8]) -> [u8; 16] {
    let hash = blake2b_simd::Params::new().hash_length(16).hash(bytes);
    let mut out = [0u8; 16];
    out.copy_from_slice(hash.as_bytes());
    out
}

/// Computes a 16-byte BLAKE2b-128 digest over the `.phb` header (excluding the
/// 16-byte checksum field at bytes `32..48`) and the full body (`64..`).
pub fn header_body_checksum(
    header_before_checksum: &[u8],
    header_after_checksum: &[u8],
    body: &[u8],
) -> [u8; 16] {
    let mut state = blake2b_simd::Params::new().hash_length(16).to_state();
    state.update(header_before_checksum);
    state.update(header_after_checksum);
    state.update(body);
    let mut out = [0u8; 16];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// Computes an 8-byte little-endian BLAKE2b checksum for a section or unit blob.
pub fn blake2b_64(bytes: &[u8]) -> u64 {
    let hash = blake2b_simd::Params::new().hash_length(8).hash(bytes);
    let mut out = [0u8; 8];
    out.copy_from_slice(hash.as_bytes());
    u64::from_le_bytes(out)
}

/// Computes the 16-byte runtime ABI hash combining format version, bytecode
/// revision, crate version, explicit opcode schema, and extension registry
/// fingerprint.
pub fn runtime_abi_hash(registry_fingerprint: [u8; 16]) -> [u8; 16] {
    let mut state = blake2b_simd::Params::new().hash_length(16).to_state();
    state.update(b"phera-aot-abi-v1\0");
    state.update(&FORMAT_VERSION.to_le_bytes());
    state.update(&BYTECODE_REVISION.to_le_bytes());
    state.update(env!("CARGO_PKG_VERSION").as_bytes());
    state.update(b"\0");
    state.update(opcodes::schema_description().as_bytes());
    state.update(b"\0");
    state.update(&registry_fingerprint);
    let hash = state.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(hash.as_bytes());
    out
}

pub(crate) fn dynamic_kind_bit(kind: DynamicKind) -> u32 {
    match kind {
        DynamicKind::Eval => 1 << 0,
        DynamicKind::DynamicInclude => 1 << 1,
        DynamicKind::DynamicNew => 1 << 2,
        DynamicKind::DynamicStaticCall => 1 << 3,
        DynamicKind::DynamicMethodCall => 1 << 4,
        DynamicKind::ClassAlias => 1 << 5,
        DynamicKind::Extract => 1 << 6,
        DynamicKind::Compact => 1 << 7,
        DynamicKind::Unserialize => 1 << 8,
        DynamicKind::Reflection => 1 << 9,
        DynamicKind::VariableVariable => 1 << 10,
    }
}

pub(crate) fn encode_allow_mask(allow: &[DynamicKind]) -> u32 {
    allow
        .iter()
        .fold(0u32, |mask, &kind| mask | dynamic_kind_bit(kind))
}

pub(crate) fn decode_allow_mask(mask: u32) -> Vec<DynamicKind> {
    const ALL: [DynamicKind; 11] = [
        DynamicKind::Eval,
        DynamicKind::DynamicInclude,
        DynamicKind::DynamicNew,
        DynamicKind::DynamicStaticCall,
        DynamicKind::DynamicMethodCall,
        DynamicKind::ClassAlias,
        DynamicKind::Extract,
        DynamicKind::Compact,
        DynamicKind::Unserialize,
        DynamicKind::Reflection,
        DynamicKind::VariableVariable,
    ];
    ALL.into_iter()
        .filter(|&kind| (mask & dynamic_kind_bit(kind)) != 0)
        .collect()
}
