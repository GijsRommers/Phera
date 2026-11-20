//! Intl break iterators expose UTF-8 byte offsets over ICU4X boundaries.
//!
//! ```php
//! $b = IntlBreakIterator::createCharacterInstance();
//! $b->setText("éa"); // 0, 2, 3
//! ```

mod cursor;
mod registration;

use std::rc::Rc;

use icu_properties::{CodePointMapData, props::Script};
use icu_segmenter::options::{LineBreakOptions, SentenceBreakOptions, WordType};
use icu_segmenter::{LineSegmenter, SentenceSegmenter, WordSegmenter};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object};
use phera_extension_api::{PhpArray, Registry, Value, args};

use crate::{error, grapheme, locale, locale_id, number};

const CLASS: &str = "IntlBreakIterator";
const RULES: &str = "IntlRuleBasedBreakIterator";
const CODE_POINT: &str = "IntlCodePointBreakIterator";

pub fn register(registry: &mut Registry) {
    registration::register(registry);
}

#[derive(Clone, Copy)]
enum Mode {
    Character,
    CodePoint,
    Line,
    Sentence,
    Word,
}

#[derive(Clone)]
struct State {
    mode: Mode,
    locale: String,
    text: Option<Rc<str>>,
    boundaries: Vec<(usize, i64)>,
    position: usize,
    codepoint: i64,
    error: (i32, Option<String>),
}

impl State {
    fn current(&self) -> usize {
        self.boundaries[self.position].0
    }

    fn position(&mut self, index: usize) -> i64 {
        self.position = index.min(self.boundaries.len() - 1);
        self.current() as i64
    }

    fn step(&mut self, distance: i64) -> i64 {
        let old = self.current();
        let position = (self.position as i64).saturating_add(distance);
        let last = self.boundaries.len() - 1;
        if position < 0 || position as usize > last {
            self.position(usize::from(position >= 0) * last);
            self.codepoint = -1;
            return -1;
        }
        let new = self.position(position as usize);
        let start = old.min(new as usize);
        self.codepoint = self
            .text
            .as_deref()
            .and_then(|text| text.get(start..))
            .and_then(|text| text.chars().next())
            .map_or(-1, |ch| i64::from(u32::from(ch)));
        new
    }
}

fn create(context: &mut dyn Context, values: &[Value], mode: Mode) -> NativeResult {
    error::clear(context);
    let state = State {
        mode,
        locale: locale::effective(context, args::bytes(values, 0)),
        text: None,
        boundaries: vec![(0, 0)],
        position: 0,
        codepoint: -1,
        error: (0, None),
    };
    let class = if matches!(mode, Mode::CodePoint) {
        CODE_POINT
    } else {
        RULES
    };
    let object = context.create_object(class, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native_payload_mut().map_err(|_| missing())? = Some(Box::new(state));
    }
    Ok(object)
}

fn update<T>(object: &Object, f: impl FnOnce(&mut State) -> T) -> Result<T, NativeError> {
    let mut data = object.native_payload_mut().map_err(|_| missing())?;
    data.as_deref_mut()
        .and_then(|data| data.downcast_mut::<State>())
        .map(f)
        .ok_or_else(missing)
}

#[derive(Clone, Copy)]
enum Op {
    Current,
    First,
    Last,
    Next,
    Previous,
    Following,
    Preceding,
    Boundary,
    GetText,
    SetText,
    Locale,
    ErrorCode,
    ErrorMessage,
    Status,
    StatusVec,
    LastCodePoint,
}

fn invoke(
    context: &mut dyn Context,
    object: Option<&Rc<Object>>,
    values: &[Value],
    op: Op,
) -> NativeResult {
    update(object.ok_or_else(missing)?, |state| {
        let offset = args::int(values, 0).unwrap_or_default();
        let boundary = |state: &State| state.boundaries[state.position].1;
        match op {
            Op::Current => Ok(Value::Int(state.current() as i64)),
            Op::First => Ok(Value::Int(state.position(0))),
            Op::Last => Ok(Value::Int(state.position(usize::MAX))),
            Op::Next => Ok(Value::Int(state.step(args::int(values, 0).unwrap_or(1)))),
            Op::Previous => Ok(Value::Int(state.step(-1))),
            Op::Following | Op::Preceding | Op::Boundary => relative(state, offset, op),
            Op::GetText => Ok(state.text.as_deref().map_or(Value::Null, Value::from)),
            Op::SetText => set_text(context, state, args::bytes(values, 0)),
            Op::Locale => {
                if !matches!(offset, 0 | 1) {
                    return error::fail(
                        context,
                        "IntlBreakIterator::getLocale",
                        1,
                        "Invalid locale type",
                        Value::Bool(false),
                    );
                }
                let name = if matches!(state.mode, Mode::CodePoint) {
                    String::new()
                } else {
                    number::resolve_locale(&state.locale)
                };
                Ok(Value::from(name))
            }
            Op::ErrorCode => Ok(Value::Int(i64::from(state.error.0))),
            Op::ErrorMessage => {
                let name = error::name(state.error.0);
                let value = state
                    .error
                    .1
                    .as_ref()
                    .map_or_else(|| name.to_owned(), |detail| format!("{detail}: {name}"));
                Ok(Value::from(value))
            }
            Op::Status => Ok(Value::Int(boundary(state))),
            Op::StatusVec => {
                let values: PhpArray = [(ArrayKey::Int(0), Value::Int(boundary(state)))]
                    .into_iter()
                    .collect();
                Ok(Value::from(values))
            }
            Op::LastCodePoint => Ok(Value::Int(state.codepoint)),
        }
    })?
}

fn relative(state: &mut State, offset: i64, op: Op) -> NativeResult {
    let argument = match op {
        Op::Following => "following",
        Op::Preceding => "preceding",
        _ => "isBoundary",
    };
    if i32::try_from(offset).is_err() {
        return Err(NativeError::value_error(format!(
            "{CLASS}::{argument}(): Argument #1 ($offset) is out of range"
        )));
    }
    let lower = state
        .boundaries
        .partition_point(|&(end, _)| (end as i64) < offset);
    let upper = state
        .boundaries
        .partition_point(|&(end, _)| (end as i64) <= offset);
    Ok(match op {
        Op::Following => {
            state.position(upper);
            Value::Int(if upper < state.boundaries.len() {
                state.current() as i64
            } else {
                -1
            })
        }
        Op::Preceding => {
            state.position(lower.saturating_sub(1));
            Value::Int(if lower > 0 {
                state.current() as i64
            } else {
                -1
            })
        }
        _ => {
            state.position(lower);
            Value::Bool(lower != upper)
        }
    })
}

fn set_text(context: &mut dyn Context, state: &mut State, value: &[u8]) -> NativeResult {
    error::clear(context);
    state.error = (0, None);
    let Ok(text) = std::str::from_utf8(value) else {
        let site = "IntlBreakIterator::setText";
        let detail = "Invalid UTF-8 text";
        state.error = (10, Some(format!("{site}(): {detail}")));
        return error::fail(context, site, 10, detail, Value::Bool(false));
    };
    let locale = locale_id::cldr(&state.locale);
    state.boundaries = match state.mode {
        Mode::Character => grapheme::boundaries(value).map(|i| (i, 0)).collect(),
        Mode::CodePoint => text
            .char_indices()
            .map(|(offset, _)| (offset, 0))
            .chain(std::iter::once((text.len(), 0)))
            .collect(),
        Mode::Line => {
            let mut options = LineBreakOptions::default();
            options.content_locale = Some(&locale.id);
            LineSegmenter::new_dictionary(options)
                .segment_str(text)
                .map(|i| {
                    let hard = i != 0
                        && (i == text.len()
                            || text[..i].ends_with(['\n', '\r', '\u{2028}', '\u{2029}']));
                    (i, if hard { 100 } else { 0 })
                })
                .collect()
        }
        Mode::Sentence => {
            let mut options = SentenceBreakOptions::default();
            options.content_locale = Some(&locale.id);
            let breaks = SentenceSegmenter::try_new(options).ok();
            if let Some(breaks) = breaks {
                breaks
                    .as_borrowed()
                    .segment_str(text)
                    .map(|i| {
                        let last = text[..i].trim_end_matches([' ', '\t']).chars().next_back();
                        let status = if last
                            .is_some_and(|ch| !matches!(ch, '.' | '!' | '?' | '。' | '！' | '？'))
                        {
                            100
                        } else {
                            0
                        };
                        (i, status)
                    })
                    .collect()
            } else {
                vec![(0, 0), (text.len(), 100)]
            }
        }
        Mode::Word => {
            let mut start = 0;
            WordSegmenter::new_dictionary(Default::default())
                .segment_str(text)
                .iter_with_word_type()
                .map(|(end, kind)| {
                    let status = match kind {
                        WordType::Number => 100,
                        WordType::Letter => {
                            let scripts = CodePointMapData::<Script>::new();
                            match text[start..end].chars().next().map(|ch| scripts.get(ch)) {
                                Some(Script::Han) => 400,
                                Some(Script::Katakana | Script::Hiragana) => 300,
                                _ => 200,
                            }
                        }
                        _ => 0,
                    };
                    start = end;
                    (end, status)
                })
                .collect()
        }
    };
    if state.boundaries.is_empty() {
        state.boundaries.push((0, 0));
    }
    state.text = Some(Rc::from(text));
    state.position = 0;
    state.codepoint = -1;
    Ok(Value::Bool(true))
}

fn missing() -> NativeError {
    NativeError::error("Found unconstructed IntlBreakIterator")
}

fn parts(context: &mut dyn Context, object: Option<&Rc<Object>>, values: &[Value]) -> NativeResult {
    let object = object.ok_or_else(missing)?;
    let mode = std::str::from_utf8(args::bytes(values, 0))
        .ok()
        .and_then(|text| text.parse::<i64>().ok())
        .or_else(|| args::int(values, 0))
        .unwrap_or_default();
    if !matches!(mode, 0..=2) {
        return Err(NativeError::value_error(
            "IntlBreakIterator::getPartsIterator(): Argument #1 ($type) is invalid",
        ));
    }
    let entries = update(object, |state| {
        let text = state.text.as_deref().unwrap_or_default();
        state
            .boundaries
            .windows(2)
            .enumerate()
            .map(|(index, pair)| {
                let (start, (end, status)) = (pair[0].0, pair[1]);
                let key = match mode {
                    1 => start,
                    2 => end,
                    _ => index,
                };
                (
                    Value::Int(key as i64),
                    Value::from(&text[start..end]),
                    status,
                )
            })
            .collect()
    })?;
    crate::iterator::parts(context, entries, object.clone())
}
