//! Read native-endian ICU ResB bundles, such as those produced by genrb.
//!
//! ```php
//! $table = new ResourceBundle("en", __DIR__ . "/messages");
//! echo $table["welcome"];
//! ```

use std::rc::Rc;

use phera_extension_api::{ArrayKey, PhpArray, Value};

use super::data::{Node, Table};

const OFFSET_MASK: u32 = 0x0fff_ffff;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 250_000;
const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;

pub struct Bundle {
    pub root: Table,
    pub no_fallback: bool,
}

pub fn parse(input: &[u8]) -> Option<Bundle> {
    let header = input.get(..24)?;
    if header[2..4] != [0xda, 0x27]
        || header[8] != u8::from(cfg!(target_endian = "big"))
        || header[9..12] != [0, 2, 0]
        || &header[12..16] != b"ResB"
        || !(1..=3).contains(&header[16])
    {
        return None;
    }
    let header_size = word(header, 0)? as usize;
    let body = input.get(header_size..)?;
    let indexes = (long(body, 4)? & 0xff) as usize;
    if !(5..=8).contains(&indexes) {
        return None;
    }
    let key_start = 4 + indexes * 4;
    let key_end = long(body, 8)? as usize * 4;
    let end = long(body, 16)? as usize * 4;
    let flags = if indexes >= 6 { long(body, 24)? } else { 0 };
    if flags & 6 != 0 || key_start > key_end || key_end > end {
        return None;
    }
    let small_end = if indexes >= 7 {
        long(body, 28)? as usize * 4
    } else {
        key_end
    };
    if small_end < key_end || small_end > end {
        return None;
    }
    let data = Data {
        body: body.get(..end)?,
        key_start,
        key_end,
        small: body.get(key_end..small_end)?,
    };
    let Node::Table(root) = data.decode(long(body, 0)?)? else {
        return None;
    };
    Some(Bundle {
        root,
        no_fallback: flags & 1 != 0,
    })
}

struct Data<'a> {
    body: &'a [u8],
    key_start: usize,
    key_end: usize,
    small: &'a [u8],
}

enum Item {
    Decode(u32, usize),
    Assemble(Vec<ArrayKey>),
}

impl Data<'_> {
    fn decode(&self, root: u32) -> Option<Node> {
        let mut todo = vec![Item::Decode(root, 0)];
        let mut values = Vec::new();
        let mut nodes = 0usize;
        let mut bytes = MAX_DECODED_BYTES;
        while let Some(item) = todo.pop() {
            let (handle, depth) = match item {
                Item::Decode(handle, depth) => (handle, depth),
                Item::Assemble(keys) => {
                    let start = values.len().checked_sub(keys.len())?;
                    let children = values.drain(start..).collect::<Vec<_>>();
                    values.push(Node::Table(Rc::new(
                        keys.into_iter().zip(children).collect(),
                    )));
                    continue;
                }
            };
            nodes += 1;
            if depth > MAX_DEPTH || nodes > MAX_NODES {
                return None;
            }
            let kind = handle >> 28;
            let offset = (handle & OFFSET_MASK) as usize;
            match kind {
                0 | 6 => {
                    let string = if offset == 0 {
                        String::new()
                    } else if kind == 6 {
                        self.short_string(offset, &mut bytes)?
                    } else {
                        let at = offset * 4;
                        utf16(self.body, at + 4, long(self.body, at)? as usize, &mut bytes)?
                    };
                    values.push(Node::Scalar(Value::from(string)));
                }
                1 => {
                    let at = offset * 4;
                    let size = if offset == 0 {
                        0
                    } else {
                        long(self.body, at)? as usize
                    };
                    spend(&mut bytes, size)?;
                    let value = self.body.get(at + 4..(at + 4).checked_add(size)?)?;
                    values.push(Node::Scalar(Value::from(value.to_vec())));
                }
                2 | 4 | 5 | 8 | 9 => {
                    let entries = self.container(kind, offset, &mut bytes)?;
                    if nodes.checked_add(todo.len())?.checked_add(entries.len())? > MAX_NODES {
                        return None;
                    }
                    let keys: Vec<_> = entries.iter().map(|(key, _)| key.clone()).collect();
                    todo.push(Item::Assemble(keys));
                    todo.extend(
                        entries
                            .into_iter()
                            .rev()
                            .map(|(_, handle)| Item::Decode(handle, depth + 1)),
                    );
                }
                7 => values.push(Node::Scalar(Value::Int(i64::from(
                    (handle << 4) as i32 >> 4,
                )))),
                14 => values.push(Node::Scalar(self.integers(offset, &mut bytes)?)),
                _ => return None,
            }
        }
        values.pop()
    }

    fn container(
        &self,
        kind: u32,
        offset: usize,
        budget: &mut usize,
    ) -> Option<Vec<(ArrayKey, u32)>> {
        if offset == 0 {
            return Some(Vec::new());
        }
        let short = matches!(kind, 5 | 9);
        let array = matches!(kind, 8 | 9);
        let source = if short { self.small } else { self.body };
        let at = offset * if short { 2 } else { 4 };
        let key_width = if matches!(kind, 2 | 5 | 9) { 2 } else { 4 };
        let count = integer(source, at, key_width)?;
        let start = at + key_width;
        let value_width = if short { 2 } else { 4 };
        let keys_end = if array {
            start
        } else {
            start.checked_add(count.checked_mul(key_width)?)?
        };
        let values_start = keys_end.next_multiple_of(value_width);
        let bytes = count.checked_mul(value_width)?;
        if count > MAX_NODES
            || source
                .get(values_start..values_start.checked_add(bytes)?)
                .is_none()
        {
            return None;
        }
        (0..count)
            .map(|index| {
                let key = if array {
                    ArrayKey::Int(index as i64)
                } else {
                    let offset = integer(source, start + index * key_width, key_width)?;
                    let key = self.key(offset)?;
                    spend(budget, key.len())?;
                    ArrayKey::str(key)
                };
                let value =
                    integer(source, values_start + index * value_width, value_width)? as u32;
                Some((key, if short { value | 0x6000_0000 } else { value }))
            })
            .collect()
    }

    fn key(&self, offset: usize) -> Option<&str> {
        if offset < self.key_start || offset >= self.key_end {
            return None;
        }
        let key = self.body.get(offset..self.key_end)?;
        let size = key.iter().position(|byte| *byte == 0)?;
        std::str::from_utf8(&key[..size]).ok()
    }

    fn short_string(&self, offset: usize, budget: &mut usize) -> Option<String> {
        let at = offset * 2;
        let first = word(self.small, at)?;
        let (start, units) = match first {
            0xdc00..=0xdfee => (at + 2, (first & 0x3ff) as usize),
            0xdfef..=0xdffe => (
                at + 4,
                ((first - 0xdfef) as usize) << 16 | word(self.small, at + 2)? as usize,
            ),
            0xdfff => (
                at + 6,
                (word(self.small, at + 2)? as usize) << 16 | word(self.small, at + 4)? as usize,
            ),
            _ => {
                let tail = self.small.get(at..)?;
                let units = tail
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .position(|unit| *unit == [0, 0])?;
                (at, units)
            }
        };
        utf16(self.small, start, units, budget)
    }

    fn integers(&self, offset: usize, budget: &mut usize) -> Option<Value> {
        let at = offset * 4;
        let count = if offset == 0 {
            0
        } else {
            long(self.body, at)? as usize
        };
        spend(budget, count.checked_mul(16)?)?;
        let length = count.checked_mul(4)?;
        let slice = self.body.get(at + 4..(at + 4).checked_add(length)?)?;
        let data = (0..count)
            .map(|index| {
                let value = long(slice, index * 4)?;
                Some((
                    ArrayKey::Int(index as i64),
                    Value::Int(i64::from(value as i32)),
                ))
            })
            .collect::<Option<PhpArray>>()?;
        Some(Value::from(data))
    }
}

fn spend(budget: &mut usize, bytes: usize) -> Option<()> {
    *budget = budget.checked_sub(bytes)?;
    Some(())
}

fn utf16(data: &[u8], start: usize, len: usize, budget: &mut usize) -> Option<String> {
    spend(budget, len.checked_mul(3)?)?;
    let bytes = len.checked_mul(2)?;
    let data = data.get(start..start.checked_add(bytes)?)?;
    let words: Vec<_> = data
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(u16::from_ne_bytes)
        .collect();
    String::from_utf16(&words).ok()
}

fn word(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(
        data.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn long(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(
        data.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn integer(data: &[u8], at: usize, size: usize) -> Option<usize> {
    if size == 2 {
        Some(word(data, at)? as usize)
    } else {
        Some(long(data, at)? as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(value: u32) -> Vec<u8> {
        let mut header = vec![0; 24];
        header[..2].copy_from_slice(&24_u16.to_ne_bytes());
        header[2..4].copy_from_slice(&[0xda, 0x27]);
        header[4..6].copy_from_slice(&20_u16.to_ne_bytes());
        header[8] = u8::from(cfg!(target_endian = "big"));
        header[10] = 2;
        header[12..16].copy_from_slice(b"ResB");
        header[16] = 1;
        for integer in [0x4000_0008_u32, 6, 8, 11, 11, 1, 0] {
            header.extend(integer.to_ne_bytes());
        }
        header.extend(b"key\0");
        for integer in [1_u32, 28, value] {
            header.extend(integer.to_ne_bytes());
        }
        header
    }

    #[test]
    fn reads_signed_values_and_rejects_invalid_offsets_and_cycles() {
        let input = bundle(0x7fff_fffd);
        let parsed = parse(&input).expect("signed resource");
        assert!(
            matches!(&parsed.root[..], [(ArrayKey::String(key), Node::Scalar(Value::Int(-3)))]
            if key.as_bytes() == b"key")
        );
        assert!(parse(&input[..input.len() - 1]).is_none());
        assert!(parse(&bundle(0x4000_0008)).is_none());
        assert!(parse(&bundle(0x0fff_ffff)).is_none());
    }
}
