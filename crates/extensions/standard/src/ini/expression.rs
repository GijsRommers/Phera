//! `parse_ini_string('flags = (E_ALL & ~E_NOTICE) | 1')` uses 32-bit operators.

use super::{Mode, parser::Parser, strings::Fragment, token::Token};
use phera_extension_api::{Context, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Or,
    And,
    Xor,
    Not,
    Complement,
    Open,
}

impl Operator {
    fn unary(self) -> bool {
        matches!(self, Self::Not | Self::Complement)
    }

    fn apply(self, left: i32, right: i32) -> i32 {
        match self {
            Self::Or => left | right,
            Self::And => left & right,
            Self::Xor => left ^ right,
            Self::Not => i32::from(right == 0),
            Self::Complement => !right,
            Self::Open => right,
        }
    }
}

enum Operand {
    Text(Fragment),
    Integer(i32),
}

impl Operand {
    fn integer(self) -> i32 {
        match self {
            Self::Integer(int) => int,
            Self::Text(value) => atoi(&value.bytes),
        }
    }

    fn value(self, mode: Mode) -> Value {
        match self {
            Self::Text(text) if mode == Mode::Typed && text.numeric => numeric(text.bytes),
            Self::Text(text) => Value::from(text.bytes),
            Self::Integer(int) if mode == Mode::Typed => Value::Int(i64::from(int)),
            Self::Integer(int) => Value::from(int.to_string()),
        }
    }
}

impl Parser<'_> {
    pub(super) fn expression(&mut self, context: &mut dyn Context) -> Result<Value, String> {
        let mut operators = Vec::new();
        let mut values = Vec::new();
        let mut open = 0usize;
        loop {
            match self.lookahead {
                Token::Punctuation(b'~') => operators.push(Operator::Complement),
                Token::Punctuation(b'!') => operators.push(Operator::Not),
                Token::Punctuation(b'(') => {
                    operators.push(Operator::Open);
                    open += 1;
                }
                _ => {
                    let Some(fragment) = self.fragments(context, false)? else {
                        return Err(self.syntax(""));
                    };
                    values.push(Operand::Text(fragment));
                    while operators.last().is_some_and(|operator| operator.unary()) {
                        reduce(&mut values, operators.pop())?;
                    }
                    loop {
                        let operation = match self.lookahead {
                            Token::Punctuation(b'|') => Operator::Or,
                            Token::Punctuation(b'&') => Operator::And,
                            Token::Punctuation(b'^') => Operator::Xor,
                            Token::Punctuation(b')') if open > 0 => {
                                while let Some(operator) = operators.pop() {
                                    if operator == Operator::Open {
                                        break;
                                    }
                                    reduce(&mut values, Some(operator))?;
                                }
                                while operators.last().is_some_and(|operator| operator.unary()) {
                                    reduce(&mut values, operators.pop())?;
                                }
                                open -= 1;
                                self.take();
                                continue;
                            }
                            _ => {
                                while let Some(operator) = operators.pop() {
                                    if operator == Operator::Open {
                                        return Err(self.syntax(", expecting ')'"));
                                    }
                                    reduce(&mut values, Some(operator))?;
                                }
                                let Some(value) = values.pop() else {
                                    return Err(self.syntax(""));
                                };
                                return Ok(value.value(self.mode));
                            }
                        };
                        // All three binary operators are left-associative and
                        // have the same precedence in Zend's INI grammar.
                        while operators
                            .last()
                            .is_some_and(|operator| *operator != Operator::Open)
                        {
                            reduce(&mut values, operators.pop())?;
                        }
                        operators.push(operation);
                        break;
                    }
                }
            }
            if operators.len() >= super::MAX_DEPTH {
                return Err("memory exhausted".into());
            }
            self.take();
        }
    }
}

fn reduce(values: &mut Vec<Operand>, operator: Option<Operator>) -> Result<(), String> {
    let operator = operator.ok_or_else(|| "syntax error".to_owned())?;
    let right = values
        .pop()
        .ok_or_else(|| "syntax error".to_owned())?
        .integer();
    let left = if operator.unary() {
        0
    } else {
        values
            .pop()
            .ok_or_else(|| "syntax error".to_owned())?
            .integer()
    };
    values.push(Operand::Integer(operator.apply(left, right)));
    Ok(())
}

fn numeric(bytes: Vec<u8>) -> Value {
    if let Ok(number) = std::str::from_utf8(&bytes) {
        if let Ok(value) = number.parse::<i64>() {
            return Value::Int(value);
        }
        if number.contains('.')
            && let Ok(value) = number.parse::<f64>()
        {
            return Value::Float(value);
        }
    }
    Value::from(bytes)
}

fn atoi(bytes: &[u8]) -> i32 {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace() && *byte != 11);
    let bytes = start.map_or(&[][..], |start| &bytes[start..]);
    let (negative, unsigned) = match bytes {
        [b'-', rest @ ..] => (true, rest),
        [b'+', rest @ ..] => (false, rest),
        _ => (false, bytes),
    };
    let length = super::patterns::prefix(unsigned, |byte| byte.is_ascii_digit());
    let magnitude = unsigned[..length].iter().fold(0u64, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(u64::from(digit - b'0'))
    });
    if negative {
        (magnitude.min(i64::MAX as u64 + 1) as i64).wrapping_neg() as i32
    } else {
        magnitude.min(i64::MAX as u64) as i32
    }
}
