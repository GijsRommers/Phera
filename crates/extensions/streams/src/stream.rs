//! One PHP stream resource carries the cursor, mode and backing descriptor.
//!
//! `$alias = $file; fclose($alias);` closes the stream for both variables.

use std::fs::{File, Metadata};
use std::io::{self, BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::sync::Arc;

use tempfile::TempPath;

use crate::io_policy::{CHUNK, IoPolicy};

pub enum Backing {
    File(BufReader<File>),
    Memory(Cursor<Vec<u8>>),
    Input(Cursor<Arc<[u8]>>),
    Output,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wrapper {
    File,
    Standard,
    Memory,
    Temp,
    Data,
    Input,
    Output,
}

pub struct Stream {
    pub backing: Backing,
    pub wrapper: Wrapper,
    pub mode: Vec<u8>,
    pub uri: Vec<u8>,
    pub position: u64,
    pub eof: bool,
    pub seekable: bool,
    pub blocked: bool,
    pub read_buffered: bool,
    pub chunk_size: usize,
    pub spill_limit: Option<u64>,
    pub temporary_path: Option<TempPath>,
    pub data_header: Option<Vec<u8>>,
    appending: bool,
    writable: bool,
}

impl Stream {
    pub fn new(backing: Backing, wrapper: Wrapper, mode: &[u8], uri: &[u8]) -> Self {
        let seekable = match &backing {
            Backing::File(file) => file.get_ref().metadata().is_ok_and(|meta| meta.is_file()),
            Backing::Output => false,
            _ => true,
        };
        Self {
            backing,
            wrapper,
            mode: mode.to_vec(),
            uri: uri.to_vec(),
            position: 0,
            eof: false,
            seekable,
            blocked: true,
            read_buffered: true,
            chunk_size: CHUNK,
            spill_limit: None,
            temporary_path: None,
            data_header: None,
            appending: mode.first() == Some(&b'a'),
            writable: mode.first() != Some(&b'r') || mode.contains(&b'+'),
        }
    }

    pub fn file(file: File, wrapper: Wrapper, mode: &[u8], uri: &[u8]) -> Self {
        Self::new(
            Backing::File(BufReader::with_capacity(CHUNK, file)),
            wrapper,
            mode,
            uri,
        )
    }

    pub fn descriptor(&self) -> Option<&File> {
        match &self.backing {
            Backing::File(file) => Some(file.get_ref()),
            _ => None,
        }
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        self.descriptor()
            .ok_or(io::ErrorKind::Unsupported)?
            .metadata()
    }

    pub fn memory_size(&self) -> Option<usize> {
        match &self.backing {
            Backing::Memory(cursor) => Some(cursor.get_ref().len()),
            _ => None,
        }
    }

    pub fn unread(&self) -> usize {
        match &self.backing {
            Backing::File(file) => file.buffer().len(),
            _ => 0,
        }
    }

    pub fn read_policy(&self, limit: Option<u64>) -> IoPolicy {
        let length = limit.or_else(|| {
            self.metadata()
                .ok()
                .map(|meta| meta.len())
                .or_else(|| self.memory_size().map(|size| size as u64))
                .or_else(|| match &self.backing {
                    Backing::Input(cursor) => Some(cursor.get_ref().len() as u64),
                    _ => None,
                })
                .map(|length| length.saturating_sub(self.position))
        });
        IoPolicy::transfer(self.seekable && self.wrapper != Wrapper::Standard, length)
    }

    pub fn write_policy(&self, length: u64) -> IoPolicy {
        let spills = self
            .spill_limit
            .is_some_and(|limit| self.position.saturating_add(length) >= limit);
        IoPolicy::transfer(
            self.seekable && !spills && self.wrapper != Wrapper::Standard,
            Some(length),
        )
    }

    pub fn read(&mut self, limit: Option<u64>) -> io::Result<Vec<u8>> {
        let all = limit.is_none();
        let limit = limit.unwrap_or(u64::MAX);
        let mut output = Vec::new();
        let mut buffer = [0; CHUNK];
        let chunk_size = self.chunk_size.min(buffer.len());
        while (output.len() as u64) < limit {
            let remaining = limit - output.len() as u64;
            let requested = remaining.min(chunk_size as u64) as usize;
            let count = match self.read_bytes(&mut buffer[..requested]) {
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            };
            if count == 0 {
                self.eof = true;
                break;
            }
            self.position += count as u64;
            output
                .try_reserve(count)
                .map_err(|_| io::ErrorKind::OutOfMemory)?;
            output.extend_from_slice(&buffer[..count]);
            if !all && !self.seekable {
                break;
            }
        }
        Ok(output)
    }

    fn read_bytes(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.backing {
            Backing::File(file) if !self.read_buffered && file.buffer().is_empty() => {
                file.get_mut().read(buffer)
            }
            Backing::File(file) => file.read(buffer),
            Backing::Memory(cursor) => cursor.read(buffer),
            Backing::Input(cursor) => cursor.read(buffer),
            Backing::Output => Ok(0),
        }
    }

    pub fn line(&mut self, limit: Option<u64>) -> io::Result<Vec<u8>> {
        let mut line = Vec::new();
        let limit = limit.unwrap_or(u64::MAX);
        match &mut self.backing {
            Backing::File(file) => file.take(limit).read_until(b'\n', &mut line)?,
            Backing::Memory(cursor) => cursor.take(limit).read_until(b'\n', &mut line)?,
            Backing::Input(cursor) => cursor.take(limit).read_until(b'\n', &mut line)?,
            Backing::Output => 0,
        };
        self.position += line.len() as u64;
        if line.last() != Some(&b'\n') && (line.len() as u64) < limit {
            self.eof = true;
        }
        Ok(line)
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.wrapper == Wrapper::Data || self.wrapper == Wrapper::Input {
            return Err(io::ErrorKind::Unsupported.into());
        }
        self.spill(bytes.len() as u64)?;
        let written = match &mut self.backing {
            Backing::File(file) => {
                if self.seekable && !file.buffer().is_empty() {
                    file.seek(SeekFrom::Start(self.position))?;
                }
                file.get_mut().write(bytes)?
            }
            Backing::Memory(cursor) if self.writable => {
                if self.appending {
                    cursor.set_position(cursor.get_ref().len() as u64);
                }
                let end = usize::try_from(cursor.position())
                    .ok()
                    .and_then(|position| position.checked_add(bytes.len()))
                    .ok_or(io::ErrorKind::OutOfMemory)?;
                let memory = cursor.get_mut();
                memory
                    .try_reserve(end.saturating_sub(memory.len()))
                    .map_err(|_| io::ErrorKind::OutOfMemory)?;
                cursor.write(bytes)?
            }
            Backing::Memory(_) | Backing::Input(_) => {
                return Err(io::ErrorKind::Unsupported.into());
            }
            Backing::Output => bytes.len(),
        };
        self.position += written as u64;
        Ok(written)
    }

    fn spill(&mut self, additional: u64) -> io::Result<()> {
        let Some(limit) = self.spill_limit else {
            return Ok(());
        };
        let Backing::Memory(memory) = &mut self.backing else {
            return Ok(());
        };
        if self.position.saturating_add(additional) < limit {
            return Ok(());
        }
        if !self.writable {
            return Err(io::ErrorKind::Unsupported.into());
        }
        let mut file = tempfile::tempfile()?;
        file.write_all(memory.get_ref())?;
        file.seek(SeekFrom::Start(memory.position()))?;
        self.backing = Backing::File(BufReader::with_capacity(CHUNK, file));
        self.spill_limit = None;
        Ok(())
    }

    pub fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if !self.seekable {
            return Err(io::ErrorKind::Unsupported.into());
        }
        let position = match &mut self.backing {
            Backing::File(file) => file.seek(position)?,
            Backing::Memory(cursor) => cursor.seek(position)?,
            Backing::Input(cursor) => cursor.seek(position)?,
            Backing::Output => return Err(io::ErrorKind::Unsupported.into()),
        };
        self.position = position;
        self.eof = false;
        Ok(position)
    }

    pub fn truncate(&mut self, size: u64) -> io::Result<()> {
        if !self.writable && matches!(self.wrapper, Wrapper::Memory | Wrapper::Temp) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        match &mut self.backing {
            Backing::File(file) => file.get_mut().set_len(size),
            Backing::Memory(cursor) => {
                let size = usize::try_from(size).map_err(|_| io::ErrorKind::OutOfMemory)?;
                let bytes = cursor.get_mut();
                bytes
                    .try_reserve(size.saturating_sub(bytes.len()))
                    .map_err(|_| io::ErrorKind::OutOfMemory)?;
                bytes.resize(size, 0);
                cursor.set_position(cursor.position().min(size as u64));
                Ok(())
            }
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }
}
