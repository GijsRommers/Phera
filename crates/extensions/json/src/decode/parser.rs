//! Explicit grammar frames avoid dependence on the native thread stack.
use super::scanner::{Scanner, Token};
use crate::{error::Error, flags::Flags};
use phera_extension_api::{ArrayKey, PhpArray, Value};
/// Object construction is injected privately until Context can create stdClass.
pub(super) type ObjectFactory = fn(PhpArray) -> Value;
/// A precise grammar state prevents trailing commas and mismatched delimiters.
enum State {
    First,
    Value,
    Key,
    Colon,
    Member,
    Comma,
}
/// One open container; properties retain insertion order and duplicate-key replacement.
struct Frame {
    object: bool,
    state: State,
    key: Option<ArrayKey>,
    values: PhpArray,
}
/// Parse a complete document; factory is used only for object mode.
pub(super) fn parse(
    bytes: &[u8],
    depth: usize,
    flags: Flags,
    factory: ObjectFactory,
) -> Result<Value, Error> {
    let mut scanner = Scanner {
        bytes,
        pos: 0,
        flags,
    };
    let mut frames: Vec<Frame> = Vec::new();
    let mut root = None;
    loop {
        let token = scanner.next()?;
        if frames.is_empty() && root.is_some() {
            return if matches!(token, Token::End) {
                root.ok_or(Error::Syntax)
            } else {
                Err(Error::Syntax)
            };
        }
        let token = if let Some(frame) = frames.last_mut() {
            let Some(token) = frame.advance(token, flags)? else {
                continue;
            };
            token
        } else {
            token
        };

        match token {
            Token::Open(object) => {
                if frames.len().saturating_add(1) >= depth {
                    return Err(Error::Depth);
                }
                frames.push(Frame {
                    object,
                    state: State::First,
                    key: None,
                    values: PhpArray::new(),
                });
            }
            Token::Close(object) => {
                let frame = frames.pop().ok_or(Error::Syntax)?;
                if object != frame.object {
                    return Err(Error::State);
                }
                let value = if object && !flags.associative {
                    factory(frame.values)
                } else {
                    Value::from(frame.values)
                };
                accept(&mut frames, &mut root, value, flags)?;
            }
            Token::Value(value) => accept(&mut frames, &mut root, value, flags)?,
            _ => return Err(Error::Syntax),
        }
    }
}
/// Attach a finished value to its parent without recursive tree rebuilding.
fn accept(
    frames: &mut [Frame],
    root: &mut Option<Value>,
    value: Value,
    flags: Flags,
) -> Result<(), Error> {
    if let Some(frame) = frames.last_mut() {
        if frame.object {
            let key = frame.key.take().ok_or(Error::Syntax)?;
            if !flags.associative
                && matches!(&key, ArrayKey::String(name) if name.as_bytes().first() == Some(&0))
            {
                return Err(Error::Property);
            }
            frame.values.insert(key, value);
        } else {
            frame.values.push(value).map_err(|_| Error::Syntax)?;
        }
        frame.state = State::Comma;
    } else {
        *root = Some(value);
    }
    Ok(())
}

impl Frame {
    /// Consume separators and keys before allowing the parser to read a value.
    fn advance(&mut self, token: Token, flags: Flags) -> Result<Option<Token>, Error> {
        match self.state {
            State::First if matches!(token, Token::Close(_)) => {}
            State::Comma => match token {
                Token::Comma => {
                    self.state = if self.object {
                        State::Key
                    } else {
                        State::Value
                    };
                    return Ok(None);
                }
                Token::Close(_) => {}
                _ => return Err(Error::Syntax),
            },
            State::Colon => {
                if !matches!(token, Token::Colon) {
                    return Err(Error::Syntax);
                }
                self.state = State::Member;
                return Ok(None);
            }
            State::Key | State::First if self.object => {
                let Token::Value(Value::String(name)) = token else {
                    return Err(Error::Syntax);
                };
                self.key = Some(if flags.associative {
                    ArrayKey::from_value(&Value::String(name)).map_err(|_| Error::Syntax)?
                } else {
                    ArrayKey::String(name)
                });
                self.state = State::Colon;
                return Ok(None);
            }
            State::Value | State::Member | State::First => {
                if matches!(token, Token::Close(_)) {
                    return Err(Error::Syntax);
                }
            }
            State::Key => return Err(Error::Syntax),
        }
        Ok(Some(token))
    }
}
