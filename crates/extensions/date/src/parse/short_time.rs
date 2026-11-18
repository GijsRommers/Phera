//! timelib's short textual-date action treats dotted seconds differently from standalone times.
//! For example, malformed `May 2 12:30.45` retains its partially parsed fields.
use super::{scan::Cursor, token::TimeToken};

pub fn read(input: &[u8], start: usize, twelve: bool) -> TimeToken {
    let mut cursor = Cursor { input, pos: start };
    let mut fields = [number(&mut cursor), number(&mut cursor), 0, 0];
    if cursor.peek() == Some(b':') || (twelve && cursor.peek() == Some(b'.')) {
        fields[2] = number(&mut cursor);
        if cursor.peek() == Some(b'.') {
            fields[3] = cursor.fraction(b".").unwrap_or(-9999999);
        }
    }
    let zone = if twelve {
        while cursor.peek().is_some_and(|b| !b"aApP".contains(&b)) {
            cursor.pos += 1;
        }
        if cursor.peek().is_some_and(|b| b.eq_ignore_ascii_case(&b'p')) && fields[0] != 12 {
            fields[0] += 12;
        } else if cursor.peek().is_some_and(|b| b.eq_ignore_ascii_case(&b'a')) && fields[0] == 12 {
            fields[0] = 0;
        }
        None
    } else {
        cursor.peek().filter(|b| *b != 0).map(|_| cursor.pos)
    };
    TimeToken { fields, zone }
}
fn number(cursor: &mut Cursor<'_>) -> i64 {
    while cursor.peek().is_some_and(|b| !b.is_ascii_digit() && b != 0) {
        cursor.pos += 1;
    }
    cursor.number(1, 2, 99).unwrap_or(-9999999)
}
