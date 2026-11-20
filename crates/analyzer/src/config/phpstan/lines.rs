//! Read indented NEON entries such as `paths: [src, 'tests with spaces']`.
//! Quoted strings keep their punctuation and embedded comment markers.

pub(super) use crate::baseline::neon::scalars::string as scalar;

pub(super) fn read(source: &str) -> Vec<&str> {
    source
        .lines()
        .map(|line| {
            let end = outside(line, '#').next().unwrap_or(line.len());
            line[..end].trim_end()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

pub(super) fn blocks<'a, 'b>(
    mut lines: &'b [&'a str],
) -> impl Iterator<Item = (&'a str, &'b [&'a str])> {
    std::iter::from_fn(move || {
        let (line, rest) = lines.split_first()?;
        let text = line.trim_start();
        let indentation = line.len() - text.len();
        let length = rest
            .iter()
            .take_while(|line| line.len() - line.trim_start().len() > indentation)
            .count();
        let (children, siblings) = rest.split_at(length);
        lines = siblings;
        Some((text, children))
    })
}

pub(super) fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    Some((key.trim(), value.trim()))
}

pub(super) fn list(value: &str, children: &[&str]) -> Vec<String> {
    if let Some(items) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let mut start = 0;
        return outside(items, ',')
            .chain(std::iter::once(items.len()))
            .filter_map(|end| {
                let item = scalar(items[start..end].trim());
                start = end + 1;
                item
            })
            .collect();
    }
    blocks(children)
        .filter_map(|(line, _)| scalar(line.strip_prefix('-')?.trim()))
        .collect()
}

fn outside(text: &str, delimiter: char) -> impl Iterator<Item = usize> {
    let mut quote = None;
    let mut characters = text.char_indices().peekable();
    std::iter::from_fn(move || {
        while let Some((index, ch)) = characters.next() {
            match quote {
                Some('"') if ch == '\\' => {
                    characters.next();
                }
                Some('\'') if ch == '\'' && characters.peek().is_some_and(|&(_, c)| c == '\'') => {
                    characters.next();
                }
                Some(end) if ch == end => quote = None,
                None if ch == delimiter => return Some(index),
                None if ch == '\'' || ch == '"' => quote = Some(ch),
                _ => {}
            }
        }
        None
    })
}
