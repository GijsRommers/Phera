//! Delimiters nest: `preg_match('{a{2}}i', 'AA') === 1`; `/a/k` warns.

#[derive(Default)]
pub(crate) struct Options {
    pub insensitive: bool,
    pub multiline: bool,
    pub dotall: bool,
    pub extended: bool,
    pub anchored: bool,
    pub dollar_end: bool,
    pub utf: bool,
    pub inline: String,
}
pub(crate) struct Pattern<'a> {
    pub body: &'a [u8],
    pub options: Options,
}

pub(crate) fn parse(pattern: &[u8]) -> Result<Pattern<'_>, String> {
    let start = pattern
        .iter()
        .position(|byte| !matches!(byte, b' ' | 9..=13))
        .ok_or("Empty regular expression")?;
    let delimiter = pattern[start];
    if delimiter.is_ascii_alphanumeric() || matches!(delimiter, b'\\' | 0) {
        return Err("Delimiter must not be alphanumeric, backslash, or NUL byte".into());
    }
    let end_delimiter = match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        _ => delimiter,
    };
    let mut cursor = start + 1;
    let mut nesting = 1;
    while cursor < pattern.len() {
        match pattern[cursor] {
            b'\\' if cursor + 1 < pattern.len() => cursor += 1,
            byte if byte == end_delimiter => {
                nesting -= 1;
                if nesting == 0 {
                    break;
                }
            }
            byte if byte == delimiter && delimiter != end_delimiter => nesting += 1,
            _ => {}
        }
        cursor += 1;
    }
    if cursor == pattern.len() {
        let matching = if delimiter == end_delimiter {
            ""
        } else {
            "matching "
        };
        return Err(format!(
            "No ending {matching}delimiter '{}' found",
            char::from(end_delimiter)
        ));
    }
    let mut options = Options::default();
    for &modifier in &pattern[cursor + 1..] {
        match modifier {
            b'i' => options.insensitive = true,
            b'm' => options.multiline = true,
            b's' => options.dotall = true,
            b'x' => options.extended = true,
            b'A' => options.anchored = true,
            b'D' => options.dollar_end = true,
            b'u' => options.utf = true,
            b'U' | b'J' | b'n' | b'r' => options.inline.push(char::from(modifier)),
            b'S' | b'X' | b' ' | b'\n' | b'\r' => {}
            0 => return Err("NUL byte is not a valid modifier".into()),
            other => return Err(format!("Unknown modifier '{}'", char::from(other))),
        }
    }
    Ok(Pattern {
        body: &pattern[start + 1..cursor],
        options,
    })
}
