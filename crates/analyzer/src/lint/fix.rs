//! Local edits turn `use App\\Unused;` into an empty line without rewriting other code.

use super::options::FixMode;
use phera_source::Span;

pub(super) struct Edit {
    pub span: Span,
    pub replacement: Vec<u8>,
}

impl Edit {
    pub fn remove(span: Span) -> Self {
        Self {
            span,
            replacement: Vec::new(),
        }
    }
}

pub(super) struct Fix {
    pub edits: Vec<Edit>,
}

impl Fix {
    pub fn safe(edits: Vec<Edit>) -> Self {
        Self { edits }
    }
}

pub(super) fn apply<'a>(
    source: &[u8],
    mode: FixMode,
    fixes: impl Iterator<Item = &'a Fix>,
) -> Option<Vec<u8>> {
    if mode == FixMode::None {
        return None;
    }
    let mut fixes: Vec<_> = fixes.filter(|fix| !fix.edits.is_empty()).collect();
    fixes.sort_by_key(|fix| fix.edits.iter().map(|edit| edit.span.start).min());
    let mut selected = std::collections::BTreeMap::<usize, &Edit>::new();
    for fix in fixes {
        let mut edits: Vec<_> = fix.edits.iter().collect();
        edits.sort_by_key(|edit| edit.span.start);
        if edits.iter().any(|edit| {
            edit.span.start > edit.span.end
                || edit.span.end > source.len()
                || selected
                    .range(..=edit.span.start)
                    .next_back()
                    .is_some_and(|(_, prev)| {
                        prev.span.end > edit.span.start || prev.span.start == edit.span.start
                    })
                || selected
                    .range(edit.span.start..)
                    .next()
                    .is_some_and(|(&next, _)| next < edit.span.end)
        }) || edits.windows(2).any(|pair| {
            pair[0].span.end > pair[1].span.start || pair[0].span.start == pair[1].span.start
        }) {
            continue;
        }
        for edit in edits {
            selected.insert(edit.span.start, edit);
        }
    }
    if selected.is_empty() {
        return None;
    }
    let mut output = Vec::with_capacity(source.len());
    let mut previous = 0;
    for edit in selected.into_values() {
        output.extend_from_slice(source.get(previous..edit.span.start)?);
        output.extend_from_slice(&edit.replacement);
        previous = edit.span.end;
    }
    output.extend_from_slice(source.get(previous..)?);
    (output != source).then_some(output)
}
