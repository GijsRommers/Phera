//! Preserve BMP palette indices while expanding packed rows or RLE commands.

pub(super) fn read(data: &[u8], width: u32, height: u32, count: usize) -> Result<Vec<u8>, String> {
    let offset = word(data, 10).ok_or("Missing BMP pixel offset")? as usize;
    let header = word(data, 14).ok_or("Missing BMP header")?;
    let depth_offset = if header == 12 { 24 } else { 28 };
    let depth = short(data, depth_offset).ok_or("Missing BMP color depth")?;
    if !matches!(depth, 1 | 4 | 8) {
        return Err("Unsupported indexed BMP color depth".into());
    }
    let body = data.get(offset..).ok_or("Missing BMP pixels")?;
    let width = width as usize;
    let height = height as usize;
    let mut result = vec![0; count];
    let compressed = header != 12 && matches!(word(data, 30), Some(1 | 2));
    if compressed {
        return rle(body, &mut result, width, height, depth).map(|()| result);
    }
    let top_down = header != 12 && word(data, 22).is_some_and(|number| (number as i32) < 0);
    let stride = (width * usize::from(depth)).div_ceil(32) * 4;
    let rows = body
        .get(..height * stride)
        .ok_or("Truncated BMP pixel rows")?;
    for (source, row) in rows.chunks_exact(stride).enumerate() {
        let y = if top_down {
            source
        } else {
            height - 1 - source
        };
        for (x, output) in result[y * width..(y + 1) * width].iter_mut().enumerate() {
            *output = match depth {
                1 => (row[x / 8] >> (7 - x % 8)) & 1,
                4 => (row[x / 2] >> ((1 - x % 2) * 4)) & 15,
                _ => row[x],
            };
        }
    }
    Ok(result)
}

fn rle(
    mut data: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    depth: u16,
) -> Result<(), String> {
    let mut x = 0usize;
    let mut from_bottom = 0usize;
    while let Some((&amount, rest)) = data.split_first() {
        let (&color, rest) = rest.split_first().ok_or("Truncated BMP RLE command")?;
        data = rest;
        match (amount, color) {
            (0, 0) => {
                from_bottom += 1;
                x = 0;
                continue;
            }
            (0, 1) => return Ok(()),
            (0, 2) => {
                let [dx, dy] = data.get(..2).ok_or("Truncated BMP RLE offset")? else {
                    return Err("Invalid BMP RLE offset".into());
                };
                x += usize::from(*dx);
                from_bottom += usize::from(*dy);
                data = &data[2..];
                if x > width || from_bottom >= height {
                    return Err("BMP RLE offset is out of bounds".into());
                }
                continue;
            }
            _ => {}
        }
        let count = usize::from(if amount == 0 { color } else { amount });
        if x + count > width || from_bottom >= height {
            return Err("BMP RLE run is out of bounds".into());
        }
        let start = (height - 1 - from_bottom) * width + x;
        let destination = &mut output[start..start + count];
        if amount != 0 {
            for (index, pixel) in destination.iter_mut().enumerate() {
                *pixel = if depth == 4 {
                    (color >> ((1 - index % 2) * 4)) & 15
                } else {
                    color
                };
            }
        } else {
            let packed = if depth == 4 { count.div_ceil(2) } else { count };
            let values = data
                .get(..packed.next_multiple_of(2))
                .ok_or("Truncated BMP RLE run")?;
            for (index, pixel) in destination.iter_mut().enumerate() {
                *pixel = if depth == 4 {
                    (values[index / 2] >> ((1 - index % 2) * 4)) & 15
                } else {
                    values[index]
                };
            }
            data = &data[values.len()..];
        }
        x += count;
    }
    Err("Missing BMP RLE terminator".into())
}

fn word(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn short(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}
