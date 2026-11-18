//! ISO-2022-JP switches between character sets with escape sequences:
//!
//! ```php
//! bin2hex(mb_convert_encoding('a日', 'ISO-2022-JP', 'UTF-8'));  // "611b2442467c1b2842"
//! //                                       a, ESC $ B, 日 as JIS row and column, ESC ( B
//! ```
//!
//! PHP converts through Shift_JIS, whose double bytes map arithmetically to
//! JIS X 0208 rows and columns; so does this module.

use super::{BAD, Substitute};
use crate::encoding::Encoding;

/// The character set an escape sequence selected.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `ESC ( B` (or `ESC ( H`): ASCII.
    Ascii,
    /// `ESC $ B` (or `ESC $ @`): JIS X 0208, two bytes per character.
    Jis,
    /// `ESC ( J`: JIS X 0201 Roman, ASCII with `¥` for `\` and `‾` for `~`.
    Roman,
}

const ESCAPE: u8 = 0x1B;
const YEN: u32 = 0xA5;
const OVERLINE: u32 = 0x203E;

fn escape(mode: Mode) -> &'static [u8] {
    match mode {
        Mode::Ascii => b"\x1b(B",
        Mode::Jis => b"\x1b$B",
        Mode::Roman => b"\x1b(J",
    }
}

fn switch(output: &mut Vec<u8>, mode: &mut Mode, to: Mode) {
    if *mode != to {
        output.extend_from_slice(escape(to));
        *mode = to;
    }
}

pub fn encode(points: &[u32], substitute: &Substitute) -> Vec<u8> {
    let mut output = Vec::new();
    let mut mode = Mode::Ascii;
    for &point in points {
        if point == YEN {
            switch(&mut output, &mut mode, Mode::Roman);
            output.push(b'\\');
            continue;
        }
        if mode == Mode::Roman {
            switch(&mut output, &mut mode, Mode::Ascii);
        }
        let mut sjis = super::encode(&[point], Encoding::Sjis, substitute);
        // A single Shift_JIS byte from 0x80 is half-width katakana, which
        // ISO-2022-JP cannot hold.
        if let [byte] = sjis[..]
            && byte >= 0x80
        {
            sjis = super::encode(&[point], Encoding::Ascii, substitute);
        }
        let mut index = 0;
        while index < sjis.len() {
            let lead = sjis[index];
            if lead < 0x80 {
                switch(&mut output, &mut mode, Mode::Ascii);
                output.push(lead);
                index += 1;
            } else if let Some(&trail) = sjis.get(index + 1) {
                switch(&mut output, &mut mode, Mode::Jis);
                output.extend(sjis_to_jis(lead, trail));
                index += 2;
            } else {
                index += 1;
            }
        }
    }
    if mode != Mode::Ascii {
        output.extend_from_slice(escape(Mode::Ascii));
    }
    output
}

/// The JIS X 0208 row and column of a Shift_JIS double byte.
fn sjis_to_jis(lead: u8, trail: u8) -> [u8; 2] {
    let mut row = (lead - if lead < 0xA0 { 0x81 } else { 0xC1 }) * 2 + 0x21;
    let column = if trail >= 0x9F {
        row += 1;
        trail - 0x7E
    } else {
        trail - if trail > 0x7E { 0x20 } else { 0x1F }
    };
    [row, column]
}

/// The Shift_JIS double byte of a JIS X 0208 row and column.
fn jis_to_sjis(row: u8, column: u8) -> [u8; 2] {
    let lead = (row - 0x21) / 2;
    let lead = lead + if lead < 0x1F { 0x81 } else { 0xC1 };
    let trail = if row & 1 == 0 {
        column + 0x7E
    } else {
        column + if column >= 0x60 { 0x20 } else { 0x1F }
    };
    [lead, trail]
}

pub fn decode(bytes: &[u8]) -> Vec<u32> {
    let mut points = Vec::new();
    let mut mode = Mode::Ascii;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == ESCAPE {
            match bytes.get(index + 1..index + 3) {
                Some(b"$B" | b"$@") => mode = Mode::Jis,
                Some(b"(B" | b"(H") => mode = Mode::Ascii,
                Some(b"(J") => mode = Mode::Roman,
                // An unknown escape is skipped by itself.
                _ => {
                    index += 1;
                    continue;
                }
            }
            index += 3;
            continue;
        }
        // Shift out and shift in have no meaning here and are dropped.
        if matches!(byte, 0x0E | 0x0F) {
            index += 1;
            continue;
        }
        match mode {
            Mode::Roman if byte == b'\\' => points.push(YEN),
            Mode::Roman if byte == b'~' => points.push(OVERLINE),
            Mode::Ascii | Mode::Roman => {
                points.push(if byte < 0x80 { u32::from(byte) } else { BAD })
            }
            Mode::Jis => {
                let Some(&column) = bytes.get(index + 1) else {
                    points.push(BAD);
                    break;
                };
                if (0x21..=0x7E).contains(&byte) && (0x21..=0x7E).contains(&column) {
                    points.extend(super::cjk::decode(
                        &jis_to_sjis(byte, column),
                        &super::cjk::SJIS,
                    ));
                } else {
                    points.push(BAD);
                }
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    points
}
