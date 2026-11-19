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
}
