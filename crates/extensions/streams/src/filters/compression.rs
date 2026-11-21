//! `zlib.deflate` keeps compression state across buckets and finishes on `fclose()`.

use std::io;

use zlib_rs::{Deflate, DeflateConfig, DeflateFlush, Inflate, InflateFlush, Status};

use super::{Flush, bounded};

const BUFFER: usize = 32 * 1024;

pub(super) enum Zlib {
    Inflate { codec: Box<Inflate>, ended: bool },
    Deflate { codec: Box<Deflate>, pending: bool },
}

impl Zlib {
    pub fn inflate(window: i64) -> Option<Self> {
        let (header, bits) = match window {
            -15..=-8 => (false, -window),
            0 | 8..=15 | 24..=31 | 40..=47 => (true, window),
            _ => return None,
        };
        Some(Self::Inflate {
            codec: Box::new(Inflate::new(header, bits as u8)),
            ended: false,
        })
    }

    pub fn deflate(level: i64, window: i64, memory: i64) -> Option<Self> {
        let window_bits = match window {
            -15..=-9 | 8..=15 | 25..=31 => window as i32,
            _ => return None,
        };
        let level = if (-1..=9).contains(&level) {
            level as i32
        } else {
            -1
        };
        let mem_level = if (1..=9).contains(&memory) {
            memory as i32
        } else {
            9
        };
        let config = DeflateConfig {
            level,
            window_bits,
            mem_level,
            ..DeflateConfig::default()
        };
        Some(Self::Deflate {
            codec: Box::new(Deflate::new_with_config(config)),
            pending: false,
        })
    }

    pub fn run(&mut self, bytes: &[u8], flush: Flush) -> io::Result<Vec<u8>> {
        let mut result = Vec::new();
        match self {
            Self::Inflate { codec, ended } => {
                if *ended {
                    return Ok(result);
                }
                let mode = if flush == Flush::Closing {
                    InflateFlush::Finish
                } else {
                    InflateFlush::SyncFlush
                };
                let mut inflate = |input: &[u8], output: &mut [u8]| {
                    let read = codec.total_in();
                    let written = codec.total_out();
                    let status = codec.decompress(input, output, mode).map_err(invalid)?;
                    Ok((
                        status,
                        (codec.total_in() - read) as usize,
                        (codec.total_out() - written) as usize,
                    ))
                };
                for chunk in bytes.chunks(BUFFER) {
                    if pump(chunk, &mut result, &mut inflate)? == Status::StreamEnd {
                        *ended = true;
                        break;
                    }
                }
                if !*ended && flush == Flush::Closing {
                    *ended = pump(&[], &mut result, &mut inflate)? == Status::StreamEnd;
                }
            }
            Self::Deflate { codec, pending } => {
                for chunk in bytes.chunks(BUFFER) {
                    deflate(codec, chunk, DeflateFlush::NoFlush, &mut result)?;
                    *pending = true;
                }
                if flush == Flush::Closing || (flush == Flush::Incremental && *pending) {
                    let mode = if flush == Flush::Closing {
                        DeflateFlush::Finish
                    } else {
                        DeflateFlush::SyncFlush
                    };
                    deflate(codec, &[], mode, &mut result)?;
                    *pending = false;
                }
            }
        }
        Ok(result)
    }
}

fn deflate(
    codec: &mut Deflate,
    input: &[u8],
    flush: DeflateFlush,
    result: &mut Vec<u8>,
) -> io::Result<()> {
    pump(input, result, |input, output| {
        let read = codec.total_in();
        let written = codec.total_out();
        let status = codec.compress(input, output, flush).map_err(invalid)?;
        Ok((
            status,
            (codec.total_in() - read) as usize,
            (codec.total_out() - written) as usize,
        ))
    })?;
    Ok(())
}

fn pump(
    mut bytes: &[u8],
    result: &mut Vec<u8>,
    mut step: impl FnMut(&[u8], &mut [u8]) -> io::Result<(Status, usize, usize)>,
) -> io::Result<Status> {
    let mut buffer = [0; BUFFER];
    loop {
        let (status, consumed, written) = step(bytes, &mut buffer)?;
        bounded(result.len() + written)?;
        result.extend_from_slice(&buffer[..written]);
        bytes = &bytes[consumed..];
        if status == Status::StreamEnd
            || consumed == 0 && written == 0
            || bytes.is_empty() && written < buffer.len()
        {
            return Ok(status);
        }
    }
}

fn invalid(error: impl std::fmt::Debug) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}"))
}
