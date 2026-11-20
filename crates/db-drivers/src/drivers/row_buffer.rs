//! PHP values from one result set, indexed by row and column.
//!
//! `$statement->fetch(PDO::FETCH_ASSOC)` and `fetch(PDO::FETCH_NUM)` can read
//! successive rows from the same buffer without imposing either shape at execute time.

use std::ops::Range;

use phera_extension_api::Value;

#[derive(Default)]
pub struct RowBuffer {
    width: usize,
    rows: usize,
    cells: Vec<Value>,
}

impl RowBuffer {
    pub fn with_capacity(width: usize, rows: usize) -> Self {
        let mut cells = Vec::new();
        if let Some(capacity) = width.checked_mul(rows) {
            let _ = cells.try_reserve_exact(capacity);
        }
        Self {
            width,
            rows: 0,
            cells,
        }
    }

    pub fn push(&mut self, row: impl Iterator<Item = Value>) {
        let end = self.cells.len() + self.width;
        self.cells.extend(row.take(self.width));
        self.cells.resize(end, Value::Null);
        self.rows += 1;
    }

    pub fn try_push<E>(&mut self, row: impl Iterator<Item = Result<Value, E>>) -> Result<(), E> {
        let start = self.cells.len();
        for cell in row.take(self.width) {
            match cell {
                Ok(value) => self.cells.push(value),
                Err(error) => {
                    self.cells.truncate(start);
                    return Err(error);
                }
            }
        }
        self.cells.resize(start + self.width, Value::Null);
        self.rows += 1;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.rows = 0;
    }

    pub fn get(&self, row: usize) -> Option<&[Value]> {
        self.cells.get(self.range(row)?)
    }

    pub fn get_mut(&mut self, row: usize) -> Option<&mut [Value]> {
        let range = self.range(row)?;
        self.cells.get_mut(range)
    }

    pub fn iter(&self) -> impl Iterator<Item = &[Value]> {
        (0..self.rows).filter_map(|row| self.get(row))
    }

    fn range(&self, row: usize) -> Option<Range<usize>> {
        if row >= self.rows {
            return None;
        }
        let start = row * self.width;
        Some(start..start + self.width)
    }
}
