//! `str_getcsv("x\r\xff", escape: '')` uses the C locale's character rules.

#[derive(Clone, Copy)]
pub(super) enum Encoding {
    Bytes,
    Ascii,
    Utf8,
}

impl Encoding {
    pub(super) fn for_locale(locale: &[u8]) -> Self {
        if locale
            .windows(5)
            .any(|part| part.eq_ignore_ascii_case(b"utf-8"))
            || locale
                .windows(4)
                .any(|part| part.eq_ignore_ascii_case(b"utf8"))
        {
            Self::Utf8
        } else if cfg!(target_env = "gnu") && matches!(locale, b"C" | b"POSIX") {
            // Glibc's C locale rejects the high byte range. Darwin's accepts it.
            Self::Ascii
        } else {
            Self::Bytes
        }
    }

    /// An invalid character consumes one byte during CSV parsing.
    pub(super) fn character_width(self, input: &[u8]) -> usize {
        self.valid_width(input).unwrap_or(1)
    }

    fn valid_width(self, input: &[u8]) -> Option<usize> {
        let first = *input.first()?;
        match self {
            Self::Bytes => Some(1),
            Self::Ascii => first.is_ascii().then_some(1),
            Self::Utf8 => {
                let width = match first {
                    0..=0x7f => return Some(1),
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return None,
                };
                std::str::from_utf8(input.get(..width)?).ok().map(|_| width)
            }
        }
    }

    /// Zend remembers the last two valid character starts, even when invalid
    /// bytes follow them. It subtracts the line-ending width from the buffer's end.
    pub(super) fn line_end(self, line: &[u8]) -> usize {
        let mut last = [0, 0];
        match self {
            Self::Bytes => {
                if let [.., previous, final_byte] = line {
                    last = [*previous, *final_byte];
                } else if let [final_byte] = line {
                    last[1] = *final_byte;
                }
            }
            Self::Ascii => {
                let mut bytes = line.iter().rev().filter(|byte| byte.is_ascii());
                last[1] = bytes.next().copied().unwrap_or_default();
                last[0] = bytes.next().copied().unwrap_or_default();
            }
            Self::Utf8 => {
                let mut position = 0;
                while let Some(&byte) = line.get(position) {
                    if let Some(width) = self.valid_width(&line[position..]) {
                        last = [last[1], byte];
                        position += width;
                    } else {
                        position += 1;
                    }
                }
            }
        }
        let ending = match last {
            [b'\r', b'\n'] => 2,
            [_, b'\r' | b'\n'] => 1,
            _ => 0,
        };
        line.len() - ending
    }
}
