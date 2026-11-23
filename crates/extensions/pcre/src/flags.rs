//! The `$flags` bits of the `preg_*` functions, and what they ask for.
//!
//! ```php
//! preg_match('/(a)(b)?/', 'a', $m, PREG_UNMATCHED_AS_NULL);  // $m = ['a', 'a', null]
//! preg_match('/b/', 'ab', $m, PREG_OFFSET_CAPTURE);          // $m = [['b', 1]]
//! ```

pub(crate) const PATTERN_ORDER: i64 = 1;
pub(crate) const SET_ORDER: i64 = 2;
pub(crate) const OFFSET_CAPTURE: i64 = 256;
pub(crate) const UNMATCHED_AS_NULL: i64 = 512;
pub(crate) const SPLIT_NO_EMPTY: i64 = 1;
pub(crate) const SPLIT_DELIM_CAPTURE: i64 = 2;
pub(crate) const SPLIT_OFFSET_CAPTURE: i64 = 4;
pub(crate) const GREP_INVERT: i64 = 1;

/// How each captured group is written into a match array.
#[derive(Clone, Copy, Default)]
pub(crate) struct CaptureFlags {
    /// `PREG_OFFSET_CAPTURE`: each group is `[text, offset]`.
    pub offsets: bool,
    /// `PREG_UNMATCHED_AS_NULL`: a group that did not take part is null,
    /// not `""`, and trailing ones are kept.
    pub unmatched_null: bool,
}

impl CaptureFlags {
    pub(crate) fn parse(value: i64) -> Self {
        Self {
            offsets: value & OFFSET_CAPTURE != 0,
            unmatched_null: value & UNMATCHED_AS_NULL != 0,
        }
    }
}

/// The shape of `$matches`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// `PREG_PATTERN_ORDER`: one list per group, of that group in every match.
    Pattern,
    /// `PREG_SET_ORDER`: one array per match.
    Set,
}

pub(crate) struct SplitFlags {
    /// `PREG_SPLIT_NO_EMPTY`: leave out empty pieces.
    pub no_empty: bool,
    /// `PREG_SPLIT_DELIM_CAPTURE`: the groups of each match become pieces too.
    pub delimiters: bool,
    /// `PREG_SPLIT_OFFSET_CAPTURE`: each piece is `[text, offset]`.
    pub offsets: bool,
}

impl SplitFlags {
    pub(crate) fn parse(value: i64) -> Self {
        Self {
            no_empty: value & SPLIT_NO_EMPTY != 0,
            delimiters: value & SPLIT_DELIM_CAPTURE != 0,
            offsets: value & SPLIT_OFFSET_CAPTURE != 0,
        }
    }
}
