//! Efficient source offset to line conversion for generated PHP.

/// Byte offsets of line beginnings; lookup is logarithmic even in generated files.
#[derive(Debug, Clone)]
pub struct LineTable {
    starts: Vec<usize>,
}
impl LineTable {
    pub fn new(source: &str) -> Self {
        Self::from_bytes(source.as_bytes())
    }

    pub fn from_bytes(source: &[u8]) -> Self {
        let mut starts = vec![0];
        starts.extend(source.iter().enumerate().filter_map(|(i, &byte)| {
            (byte == b'\n' || (byte == b'\r' && source.get(i + 1) != Some(&b'\n'))).then_some(i + 1)
        }));
        Self { starts }
    }
    pub fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|&s| s <= offset).max(1)
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
