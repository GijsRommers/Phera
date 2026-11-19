//! Gzip files are ordinary PHP stream resources, including transparent plain reads.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};

use flate2::{Compression, GzBuilder, bufread::MultiGzDecoder, write::GzEncoder};

use crate::io_policy::CHUNK;

mod functions;
mod open;
pub use functions::register;
pub(crate) use open::open_gzip;

pub enum Gzip {
    Reader(Box<BufReader<GzipInput>>),
    Writer(Writer),
}

pub enum GzipInput {
    Plain(BufReader<File>),
    Compressed(MultiGzDecoder<BufReader<File>>),
}

impl GzipInput {
    fn detect(file: File) -> io::Result<Self> {
        let mut reader = BufReader::new(file);
        if reader.fill_buf()?.starts_with(&[0x1f, 0x8b]) {
            Ok(Self::Compressed(MultiGzDecoder::new(reader)))
        } else {
            Ok(Self::Plain(reader))
        }
    }

    fn file(&self) -> &File {
        match self {
            Self::Plain(reader) => reader.get_ref(),
            Self::Compressed(reader) => reader.get_ref().get_ref(),
        }
    }
}

impl Read for GzipInput {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(reader) => reader.read(bytes),
            Self::Compressed(reader) => reader.read(bytes),
        }
    }
}

pub struct Writer(Option<GzEncoder<File>>);

impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(mut writer) = self.0.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn_blocking(move || {
                let _ = writer.try_finish();
            });
        }
    }
}

impl Writer {
    fn encoder(&mut self) -> io::Result<&mut GzEncoder<File>> {
        self.0
            .as_mut()
            .ok_or_else(|| io::ErrorKind::BrokenPipe.into())
    }

    pub fn finish(&mut self) -> io::Result<()> {
        match self.0.take() {
            Some(writer) => writer.finish().map(|_| ()),
            None => Ok(()),
        }
    }

    fn advance(&mut self, mut remaining: u64) -> io::Result<()> {
        let output = self.encoder()?;
        let zeros = [0; CHUNK];
        while remaining != 0 {
            let count = remaining.min(CHUNK as u64) as usize;
            output.write_all(&zeros[..count])?;
            remaining -= count as u64;
        }
        Ok(())
    }
}

impl Gzip {
    pub fn reader(file: File) -> io::Result<Self> {
        Ok(Self::Reader(Box::new(BufReader::with_capacity(
            CHUNK,
            GzipInput::detect(file)?,
        ))))
    }

    pub fn writer(file: File, level: u32) -> Self {
        let os = if cfg!(target_os = "macos") { 19 } else { 3 };
        let writer = GzBuilder::new()
            .operating_system(os)
            .write(file, Compression::new(level));
        Self::Writer(Writer(Some(writer)))
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Reader(reader) => reader.read(bytes),
            Self::Writer(_) => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    pub fn line(&mut self, length: u64, out: &mut Vec<u8>) -> io::Result<usize> {
        match self {
            Self::Reader(reader) => reader.take(length).read_until(b'\n', out),
            Self::Writer(_) => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Reader(_) => Err(io::ErrorKind::Unsupported.into()),
            Self::Writer(writer) => writer.encoder()?.write(bytes),
        }
    }

    pub fn unread(&self) -> usize {
        match self {
            Self::Reader(reader) => reader.buffer().len(),
            Self::Writer(_) => 0,
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Reader(_) => Ok(()),
            Self::Writer(writer) => writer.encoder()?.flush(),
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        match self {
            Self::Reader(_) => Ok(()),
            Self::Writer(writer) => writer.finish(),
        }
    }

    pub fn seek(&mut self, current: u64, position: SeekFrom) -> io::Result<u64> {
        let offset = match position {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(offset) => current.checked_add_signed(offset),
            SeekFrom::End(_) => None,
        }
        .ok_or(io::ErrorKind::InvalidInput)?;
        match self {
            Self::Writer(writer) => writer.advance(
                offset
                    .checked_sub(current)
                    .ok_or(io::ErrorKind::InvalidInput)?,
            )?,
            Self::Reader(reader) => {
                let from = if offset < current {
                    let mut file = reader.get_ref().file().try_clone()?;
                    file.seek(SeekFrom::Start(0))?;
                    **reader = BufReader::with_capacity(CHUNK, GzipInput::detect(file)?);
                    0
                } else {
                    current
                };
                // zlib accepts a seek past EOF. No sparse decompression state is needed.
                io::copy(&mut reader.take(offset - from), &mut io::sink())?;
            }
        }
        Ok(offset)
    }
}
