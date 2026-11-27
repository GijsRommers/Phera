//! Error types for `.phb` AOT image encoding, decoding, and validation.

use std::fmt;

/// Failure while writing, opening, verifying, or decoding a `.phb` image.
#[derive(Debug)]
pub enum ImageError {
    Io(std::io::Error),
    InvalidMagic,
    UnsupportedVersion {
        found: u16,
        expected: u16,
    },
    AbiMismatch {
        found: [u8; 16],
        expected: [u8; 16],
    },
    ChecksumMismatch,
    SectionChecksumMismatch {
        section: &'static str,
    },
    UnitChecksumMismatch {
        unit: u32,
    },
    RootMismatch {
        image_root: String,
        expected_root: String,
    },
    Corrupt(String),
    Validation(String),
    Encode(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::InvalidMagic => write!(f, "not a Phera AOT image (.phb): invalid file header"),
            Self::UnsupportedVersion { found, expected } => write!(
                f,
                "unsupported Phera AOT image format version {found} (this binary supports version {expected}); rebuild with `phera build`"
            ),
            Self::AbiMismatch { .. } => write!(
                f,
                "Phera AOT image ABI mismatch: the image was built with a different Phera version, OS, bytecode revision, or extension inventory; rebuild with `phera build`"
            ),
            Self::ChecksumMismatch => write!(
                f,
                "Phera AOT image failed BLAKE2b integrity check (corrupted or truncated file)"
            ),
            Self::SectionChecksumMismatch { section } => write!(
                f,
                "Phera AOT image section `{section}` failed checksum verification"
            ),
            Self::UnitChecksumMismatch { unit } => write!(
                f,
                "Phera AOT image unit #{unit} failed checksum verification"
            ),
            Self::RootMismatch {
                image_root,
                expected_root,
            } => write!(
                f,
                "Phera AOT image was built for `{image_root}` and cannot serve `{expected_root}`; rebuild for the target project root"
            ),
            Self::Corrupt(detail) => write!(f, "corrupt Phera AOT image: {detail}"),
            Self::Validation(detail) => {
                write!(f, "Phera AOT image failed bytecode validation: {detail}")
            }
            Self::Encode(detail) => write!(f, "failed to encode Phera AOT image: {detail}"),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ImageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
