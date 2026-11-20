//! `imageinterlace($image, true)` makes `imagepng` write the seven Adam7 passes.

use std::io::{self, Write};

use flate2::{Compression, write::ZlibEncoder};
use png::{Writer, chunk};

use super::Options;

/// Each pass contains a subset of the image, given by its first column and row
/// and the distance between samples on each axis.
const PASSES: [(usize, usize, usize, usize); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];

pub(super) fn write(
    writer: &mut Writer<impl Write>,
    width: u32,
    height: u32,
    pixels: &[u8],
    options: Options,
) -> io::Result<()> {
    let (width, height) = (width as usize, height as usize);
    let area = width * height;
    let bytes_per_pixel = pixels
        .len()
        .checked_div(area)
        .filter(|samples| matches!(samples, 1 | 3 | 4) && *samples * area == pixels.len())
        .ok_or_else(|| io::Error::other("Invalid GD image buffer"))?;
    let level = u32::try_from(options.quality)
        .ok()
        .filter(|level| *level <= 9)
        .unwrap_or(6);
    let mut encoder = ZlibEncoder::new(Idat(writer), Compression::new(level));
    let filters = selected_filters(options.filters);
    let mut row = Vec::new();
    let mut previous = Vec::new();
    let mut candidate = Vec::new();
    let mut best = Vec::new();

    for (left, top, column_step, row_step) in PASSES {
        if left >= width || top >= height {
            continue;
        }
        previous.clear();
        for y in (top..height).step_by(row_step) {
            row.clear();
            for x in (left..width).step_by(column_step) {
                let start = (y * width + x) * bytes_per_pixel;
                row.extend_from_slice(&pixels[start..start + bytes_per_pixel]);
            }
            let mut lowest_score = u32::MAX;
            for filter in filters.iter().copied() {
                let score = filter.row(&row, &previous, bytes_per_pixel, &mut candidate);
                if score < lowest_score {
                    std::mem::swap(&mut best, &mut candidate);
                    lowest_score = score;
                }
            }
            encoder.write_all(&best)?;
            std::mem::swap(&mut previous, &mut row);
        }
    }
    encoder.finish()?;
    Ok(())
}

struct Idat<'a, W: Write>(&'a mut Writer<W>);

impl<W: Write> Write for Idat<'_, W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0
            .write_chunk(chunk::IDAT, data)
            .map_err(io::Error::other)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum RowFilter {
    None,
    Sub,
    Up,
    Average,
    Paeth,
}

fn selected_filters(mask: i64) -> Vec<RowFilter> {
    if mask == 0 {
        return vec![RowFilter::None];
    }
    let filters = [
        RowFilter::None,
        RowFilter::Sub,
        RowFilter::Up,
        RowFilter::Average,
        RowFilter::Paeth,
    ];
    let selected: Vec<_> = filters
        .into_iter()
        .filter(|filter| mask < 0 || mask & (8 << *filter as u8) != 0)
        .collect();
    if selected.is_empty() {
        vec![RowFilter::None]
    } else {
        selected
    }
}

impl RowFilter {
    fn row(self, row: &[u8], previous: &[u8], samples: usize, output: &mut Vec<u8>) -> u32 {
        output.clear();
        output.push(self as u8);
        let mut score = 0;
        for (index, &byte) in row.iter().enumerate() {
            let above = previous.get(index).copied().unwrap_or_default();
            let left = index.checked_sub(samples);
            let before = left.map_or(0, |left| row[left]);
            let corner = left
                .and_then(|left| previous.get(left))
                .copied()
                .unwrap_or(0);
            let predicted = match self {
                Self::None => 0,
                Self::Sub => before,
                Self::Up => above,
                Self::Average => before.midpoint(above),
                Self::Paeth => paeth(before, above, corner),
            };
            let residual = byte.wrapping_sub(predicted);
            output.push(residual);
            score += u32::from((residual as i8).unsigned_abs());
        }
        score
    }
}

fn paeth(before: u8, above: u8, corner: u8) -> u8 {
    let (a, b, c) = (i16::from(before), i16::from(above), i16::from(corner));
    let [first, second, third] = [(b - c).abs(), (a - c).abs(), (a + b - 2 * c).abs()];
    if first <= second && first <= third {
        before
    } else if second <= third {
        above
    } else {
        corner
    }
}
