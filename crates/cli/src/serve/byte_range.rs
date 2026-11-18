//! The `Range` header: sending part of a file.
//!
//! ```text
//! Range: bytes=0-499      the first 500 bytes
//! Range: bytes=500-       everything from byte 500 on
//! Range: bytes=-200       the last 200 bytes
//! ```
//!
//! Video players seek with it and download tools resume with it.

/// What a `Range` header asks for, given the length of the file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RangeRequest {
    /// Send everything: there is no header, or one this server does not
    /// honour (another unit, several ranges at once, a malformed range).
    /// HTTP allows ignoring those.
    Whole,
    /// Send `length` bytes starting at byte `start`.
    Part { start: u64, length: u64 },
    /// The range lies past the end of the file: the answer is 416.
    Unsatisfiable,
}

pub(crate) fn parse(header: Option<&str>, file_length: u64) -> RangeRequest {
    let Some((unit, range)) = header.and_then(|value| value.split_once('=')) else {
        return RangeRequest::Whole;
    };
    if !unit.trim().eq_ignore_ascii_case("bytes") || range.contains(',') {
        return RangeRequest::Whole;
    }
    let Some((first, last)) = range.split_once('-') else {
        return RangeRequest::Whole;
    };
    match (position(first), position(last)) {
        // `-200`: the last 200 bytes, or the whole file if it is shorter.
        (Position::Absent, Position::At(count)) => {
            if count == 0 || file_length == 0 {
                return RangeRequest::Unsatisfiable;
            }
            let start = file_length.saturating_sub(count);
            part(start, file_length - 1, file_length)
        }
        (Position::At(first), Position::Absent) => part(first, u64::MAX, file_length),
        (Position::At(first), Position::At(last)) if first <= last => {
            part(first, last, file_length)
        }
        _ => RangeRequest::Whole,
    }
}

/// Bytes `first` to `last`, both included, cut off at the end of the file.
fn part(first: u64, last: u64, file_length: u64) -> RangeRequest {
    if first >= file_length {
        return RangeRequest::Unsatisfiable;
    }
    let last = last.min(file_length - 1);
    RangeRequest::Part {
        start: first,
        length: last - first + 1,
    }
}

/// One side of `first-last`.
enum Position {
    Absent,
    At(u64),
    Malformed,
}

fn position(text: &str) -> Position {
    let text = text.trim();
    if text.is_empty() {
        return Position::Absent;
    }
    // Digits only: `parse` alone would also take a leading plus sign.
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Position::Malformed;
    }
    text.parse().map_or(Position::Malformed, Position::At)
}

#[cfg(test)]
mod tests {
    use super::RangeRequest::{Part, Unsatisfiable, Whole};
    use super::*;

    #[test]
    fn the_three_forms_of_a_range() {
        assert_eq!(
            parse(Some("bytes=0-499"), 1000),
            Part {
                start: 0,
                length: 500
            }
        );
        assert_eq!(
            parse(Some("bytes=500-"), 1000),
            Part {
                start: 500,
                length: 500
            }
        );
        assert_eq!(
            parse(Some("bytes=-200"), 1000),
            Part {
                start: 800,
                length: 200
            }
        );
        assert_eq!(
            parse(Some("BYTES = 9 - 9"), 10),
            Part {
                start: 9,
                length: 1
            }
        );
    }

    #[test]
    fn a_range_is_cut_off_at_the_end_of_the_file() {
        assert_eq!(
            parse(Some("bytes=900-5000"), 1000),
            Part {
                start: 900,
                length: 100
            }
        );
        assert_eq!(
            parse(Some("bytes=-5000"), 1000),
            Part {
                start: 0,
                length: 1000
            }
        );
    }

    #[test]
    fn a_range_past_the_end_cannot_be_satisfied() {
        assert_eq!(parse(Some("bytes=1000-"), 1000), Unsatisfiable);
        assert_eq!(parse(Some("bytes=1000-2000"), 1000), Unsatisfiable);
        assert_eq!(parse(Some("bytes=-0"), 1000), Unsatisfiable);
        assert_eq!(parse(Some("bytes=0-"), 0), Unsatisfiable);
        assert_eq!(parse(Some("bytes=-5"), 0), Unsatisfiable);
    }

    #[test]
    fn what_is_not_understood_is_ignored() {
        for header in [
            "bytes=0-1,5-6",
            "items=0-1",
            "bytes=5-2",
            "bytes=a-b",
            "bytes=+1-2",
            "bytes=-",
            "bytes=1",
            "bytes",
            "bytes=99999999999999999999999-",
            "",
        ] {
            assert_eq!(parse(Some(header), 1000), Whole, "{header}");
        }
        assert_eq!(parse(None, 1000), Whole);
    }
}
