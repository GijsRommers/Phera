//! `dechunk` removes HTTP chunk sizes and framing across input bucket boundaries.

use super::Flush;

#[derive(Default)]
pub(super) struct Dechunk {
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Start,
    Size {
        size: usize,
        extension: bool,
    },
    Body(usize),
    EndOfLine,
    Trailer,
    Passthrough,
}

impl Dechunk {
    pub fn run(&mut self, data: Vec<u8>, _: Flush) -> Vec<u8> {
        let mut output = Vec::new();
        for byte in data {
            self.state = match self.state {
                State::Start => match hex(byte) {
                    Some(size) => State::Size {
                        size,
                        extension: false,
                    },
                    None => {
                        output.push(byte);
                        State::Passthrough
                    }
                },
                State::Size { size, extension } => match byte {
                    b'\n' if size == 0 => State::Trailer,
                    b'\n' => State::Body(size),
                    b';' | b'\r' | b' ' | b'\t' => State::Size {
                        size,
                        extension: true,
                    },
                    _ if extension => State::Size { size, extension },
                    _ => match hex(byte).and_then(|digit| size.checked_mul(16)?.checked_add(digit))
                    {
                        Some(size) => State::Size { size, extension },
                        None => State::Passthrough,
                    },
                },
                State::Body(left) => {
                    output.push(byte);
                    if left == 1 {
                        State::EndOfLine
                    } else {
                        State::Body(left - 1)
                    }
                }
                State::EndOfLine => match byte {
                    b'\r' => State::EndOfLine,
                    b'\n' => State::Start,
                    _ => {
                        output.push(byte);
                        State::Passthrough
                    }
                },
                State::Trailer => State::Trailer,
                State::Passthrough => {
                    output.push(byte);
                    State::Passthrough
                }
            };
        }
        output
    }
}

pub(super) fn hex(byte: u8) -> Option<usize> {
    Some(match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => return None,
    } as usize)
}
