//! The JSON grammar over the tokens, with a stack of open containers
//! instead of recursion, so that `json_decode(str_repeat('[', 100000) ...)`
//! cannot overflow the Rust stack.
//!
//! ```php
//! json_decode('[1,]');                // null, JSON_ERROR_SYNTAX
//! json_decode('[}');                  // null, JSON_ERROR_STATE_MISMATCH
//! json_decode('{"a":1,"a":2}', true); // ['a' => 2]: a repeated key replaces
//! json_decode('[[1]]', true, 2);      // null, JSON_ERROR_DEPTH
//! ```

use super::scanner::{Container, Scanner, Token};
use crate::{error::Error, flags::DecodeFlags};
use phera_extension_api::{ArrayKey, PhpArray, Value};

/// Turns the members of a JSON object into a PHP object. The parser does
/// not know how objects are made: the caller passes this in.
pub(super) type ObjectFactory<'a> = &'a mut dyn FnMut(PhpArray) -> Value;
/// What an open container expects next.
enum State {
    /// The first entry, or the closing bracket of an empty container.
    First,
    /// An array entry, after a comma.
    Value,
    /// An object key, after a comma.
    Key,
    /// The colon after a key.
    Colon,
    /// The value after a colon.
    Member,
    /// A comma, or the closing bracket.
    Comma,
}

/// An open container.
struct Frame {
    container: Container,
    state: State,
    values: Option<Members>,
}

#[derive(Default)]
struct Members {
    key: Option<ArrayKey>,
    entries: PhpArray,
}

/// Parses a whole document. `factory` makes the objects, unless
/// `flags.associative` asks for arrays.
pub(super) fn parse(
    bytes: &[u8],
    depth: usize,
    flags: DecodeFlags,
    mut factory: Option<ObjectFactory<'_>>,
) -> Result<Value, Error> {
    let mut scanner = Scanner::new(bytes, flags, factory.is_some());
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
            Token::Open(container) => {
                if frames.len().saturating_add(1) >= depth {
                    return Err(Error::Depth);
                }
                frames.push(Frame {
                    container,
                    state: State::First,
                    values: factory.is_some().then(Members::default),
                });
            }
            Token::Close(container) => {
                let frame = frames.pop().ok_or(Error::Syntax)?;
                if container != frame.container {
                    return Err(Error::State);
                }
                let value = match (frame.values, &mut factory) {
                    (Some(members), Some(factory))
                        if container == Container::Object && !flags.associative =>
                    {
                        factory(members.entries)
                    }
                    (Some(members), _) => Value::from(members.entries),
                    (None, _) => Value::Null,
                };
                accept(&mut frames, &mut root, value, flags)?;
            }
            Token::String(string) => {
                accept(
                    &mut frames,
                    &mut root,
                    string.map_or(Value::Null, Value::from),
                    flags,
                )?;
            }
            Token::Value(value) => accept(&mut frames, &mut root, value, flags)?,
            _ => return Err(Error::Syntax),
        }
    }
}

/// Adds a finished value to the open container, or makes it the document.
fn accept(
    frames: &mut [Frame],
    root: &mut Option<Value>,
    value: Value,
    flags: DecodeFlags,
) -> Result<(), Error> {
    if let Some(frame) = frames.last_mut() {
        if let Some(members) = &mut frame.values {
            if frame.container == Container::Object {
                let key = members.key.take().ok_or(Error::Syntax)?;
                if !flags.associative
                    && matches!(&key, ArrayKey::String(name) if name.as_bytes().first() == Some(&0))
                {
                    return Err(Error::Property);
                }
                members.entries.insert(key, value);
            } else {
                members.entries.push(value).map_err(|_| Error::Syntax)?;
            }
        }
        frame.state = State::Comma;
    } else {
        *root = Some(value);
    }
    Ok(())
}

impl Frame {
    /// Takes the token if it is a comma, colon or key, which only move the
    /// state on; returns it if it is for the parser (a value, an opening or
    /// a closing bracket); fails if it does not belong here.
    fn advance(&mut self, token: Token, flags: DecodeFlags) -> Result<Option<Token>, Error> {
        match self.state {
            State::First if matches!(token, Token::Close(_)) => {}
            State::Comma => match token {
                Token::Comma => {
                    self.state = if self.container == Container::Object {
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
            State::Key | State::First if self.container == Container::Object => {
                let Token::String(name) = token else {
                    return Err(Error::Syntax);
                };
                if let (Some(members), Some(name)) = (&mut self.values, name) {
                    let name = name.into();
                    members.key = Some(if flags.associative {
                        ArrayKey::from_value(&Value::String(name)).map_err(|_| Error::Syntax)?
                    } else {
                        ArrayKey::String(name)
                    });
                }
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
