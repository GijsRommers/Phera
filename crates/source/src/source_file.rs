//! Efficient source offset to line conversion for generated PHP.

/// Byte offsets of line beginnings; lookup is logarithmic even in generated files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineTable {
    starts: Vec<usize>,
    first_line: usize,
}
impl LineTable {
    pub fn new(source: &str) -> Self {
        Self::from_bytes(source.as_bytes())
    }

    pub fn from_bytes(source: &[u8]) -> Self {
        Self::fragment(source, 1)
    }

    /// Preserve original line numbers when compiling part of a source file.
    pub fn fragment(source: &[u8], first_line: usize) -> Self {
        let mut starts = vec![0];
        starts.extend(source.iter().enumerate().filter_map(|(i, &byte)| {
            (byte == b'\n' || (byte == b'\r' && source.get(i + 1) != Some(&b'\n'))).then_some(i + 1)
        }));
        Self { starts, first_line }
    }
    pub fn line(&self, offset: usize) -> usize {
        self.first_line
            + self
                .starts
                .partition_point(|&s| s <= offset)
                .saturating_sub(1)
    }

    pub fn starts(&self) -> &[usize] {
        &self.starts
    }

    pub fn first_line(&self) -> usize {
        self.first_line
    }

    /// Reconstructs a validated line table from serialized parts.
    pub fn from_parts(starts: Vec<usize>, first_line: usize) -> Option<Self> {
        if starts.first() != Some(&0) {
            return None;
        }
        if starts.windows(2).any(|pair| pair[0] >= pair[1]) {
            return None;
        }
        Some(Self { starts, first_line })
    }
}
#[cfg(test)]
mod tests {
    use super::LineTable;
    #[test]
    fn byte_offsets() {
        let t = LineTable::new("é\nx\n");
        assert_eq!(t.line(0), 1);
        assert_eq!(t.line(2), 1);
        assert_eq!(t.line(3), 2);
        assert_eq!(t.line(5), 3);
    }

    #[test]
    fn from_parts_validates_and_round_trips() {
        let original = LineTable::fragment(b"a\nb\rc\r\nd", 10);
        let restored = LineTable::from_parts(original.starts().to_vec(), original.first_line())
            .expect("valid line table parts");
        assert_eq!(restored, original);
        assert!(LineTable::from_parts(Vec::new(), 1).is_none());
        assert!(LineTable::from_parts(vec![1], 1).is_none());
        assert!(LineTable::from_parts(vec![0, 5, 5], 1).is_none());
        assert!(LineTable::from_parts(vec![0, 5, 4], 1).is_none());
    }
}
