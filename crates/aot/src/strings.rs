//! Deduplicated byte/string pool shared across all sections and units of a `.phb` image.

use std::collections::HashMap;

use crate::error::ImageError;

/// Index into the `STRINGS` section of a `.phb` image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StringId(pub u32);

/// Builds a deduplicated byte arena in deterministic insertion order.
#[derive(Default)]
pub struct StringPoolBuilder {
    index: HashMap<Vec<u8>, StringId>,
    entries: Vec<Vec<u8>>,
}

impl StringPoolBuilder {
    pub fn intern_bytes(&mut self, bytes: &[u8]) -> Result<StringId, ImageError> {
        if let Some(&id) = self.index.get(bytes) {
            return Ok(id);
        }
        let id = u32::try_from(self.entries.len())
            .map(StringId)
            .map_err(|_| ImageError::Encode("string pool exceeds u32::MAX entries".into()))?;
        let owned = bytes.to_vec();
        self.index.insert(owned.clone(), id);
        self.entries.push(owned);
        Ok(id)
    }

    pub fn intern_str(&mut self, text: &str) -> Result<StringId, ImageError> {
        self.intern_bytes(text.as_bytes())
    }

    /// Encodes the `STRINGS` section:
    /// `[count: u32, offsets: [u64; count + 1], data: [u8]]`.
    pub fn encode(&self) -> Result<Vec<u8>, ImageError> {
        let count = u32::try_from(self.entries.len())
            .map_err(|_| ImageError::Encode("string pool count overflow".into()))?;
        let total_data: usize = self.entries.iter().map(Vec::len).sum();
        let mut out = Vec::with_capacity(4 + (self.entries.len() + 1) * 8 + total_data);
        out.extend_from_slice(&count.to_le_bytes());
        let mut cursor: u64 = 0;
        out.extend_from_slice(&cursor.to_le_bytes());
        for entry in &self.entries {
            cursor = cursor
                .checked_add(entry.len() as u64)
                .ok_or_else(|| ImageError::Encode("string pool byte length overflow".into()))?;
            out.extend_from_slice(&cursor.to_le_bytes());
        }
        for entry in &self.entries {
            out.extend_from_slice(entry);
        }
        Ok(out)
    }
}

/// Decoded immutable `STRINGS` section held by an open `.phb` image.
#[derive(Debug)]
pub struct StringPool {
    offsets: Box<[u64]>,
    data: Box<[u8]>,
}

impl StringPool {
    pub fn decode(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() < 4 {
            return Err(ImageError::Corrupt("STRINGS section truncated".into()));
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let offsets_len = count
            .checked_add(1)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| ImageError::Corrupt("STRINGS offset table size overflow".into()))?;
        let data_start = 4usize
            .checked_add(offsets_len)
            .ok_or_else(|| ImageError::Corrupt("STRINGS header overflow".into()))?;
        if bytes.len() < data_start {
            return Err(ImageError::Corrupt("STRINGS offset table truncated".into()));
        }
        let data_len = (bytes.len() - data_start) as u64;
        let mut offsets = Vec::with_capacity(count + 1);
        let mut prev = 0u64;
        for i in 0..=count {
            let at = 4 + i * 8;
            let value = u64::from_le_bytes([
                bytes[at],
                bytes[at + 1],
                bytes[at + 2],
                bytes[at + 3],
                bytes[at + 4],
                bytes[at + 5],
                bytes[at + 6],
                bytes[at + 7],
            ]);
            if i == 0 && value != 0 {
                return Err(ImageError::Corrupt("STRINGS first offset must be 0".into()));
            }
            if value < prev || value > data_len {
                return Err(ImageError::Corrupt(
                    "STRINGS offset table is not monotonic or exceeds payload".into(),
                ));
            }
            prev = value;
            offsets.push(value);
        }
        Ok(Self {
            offsets: offsets.into_boxed_slice(),
            data: bytes[data_start..].into(),
        })
    }

    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_bytes(&self, id: StringId) -> Result<&[u8], ImageError> {
        let idx = id.0 as usize;
        let Some(&start) = self.offsets.get(idx) else {
            return Err(ImageError::Corrupt(format!(
                "string id {} out of range (pool len {})",
                id.0,
                self.len()
            )));
        };
        let Some(&end) = self.offsets.get(idx + 1) else {
            return Err(ImageError::Corrupt(format!(
                "string id {} end offset out of range",
                id.0
            )));
        };
        let start = usize::try_from(start)
            .map_err(|_| ImageError::Corrupt("string offset exceeds usize".into()))?;
        let end = usize::try_from(end)
            .map_err(|_| ImageError::Corrupt("string end offset exceeds usize".into()))?;
        self.data
            .get(start..end)
            .ok_or_else(|| ImageError::Corrupt("string slice out of bounds".into()))
    }

    pub fn get_str(&self, id: StringId) -> Result<&str, ImageError> {
        let bytes = self.get_bytes(id)?;
        std::str::from_utf8(bytes)
            .map_err(|_| ImageError::Corrupt(format!("string id {} is not valid UTF-8", id.0)))
    }
}
