//! A generated message such as `'#^Function foo not found\.$#'` is a
//! quoted literal, so checking a baseline requires no regex engine.

pub(crate) fn string(value: &str) -> Option<String> {
    if let Some(quoted) = value.strip_prefix('\'') {
        let inner = quoted.strip_suffix('\'')?;
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\'' && chars.next() != Some('\'') {
                return None;
            }
            output.push(ch);
        }
        Some(output)
    } else if let Some(quoted) = value.strip_prefix('"') {
        let inner = quoted.strip_suffix('"')?;
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => output.push(match chars.next()? {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    _ => return None,
                }),
                '"' => return None,
                other => output.push(other),
            }
        }
        Some(output)
    } else {
        (!value.is_empty()).then(|| value.to_string())
    }
}

pub(super) fn literal_pattern(value: &str) -> Option<String> {
    const SPECIAL: &str = ".\\+*?[^]$(){}=!<>|:-#";
    let mut chars = value.strip_prefix("#^")?.strip_suffix("$#")?.chars();
    let mut output = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let escaped = chars.next()?;
                if !SPECIAL.contains(escaped) {
                    return None;
                }
                output.push(escaped);
            }
            ch if SPECIAL.contains(ch) => return None,
            ch => output.push(ch),
        }
    }
    Some(output)
}
