//! One PHP stream resource carries the cursor, mode and backing descriptor.
//!
//! `$alias = $file; fclose($alias);` closes the stream for both variables.

use std::fs::{File, Metadata};
use std::io::{self, BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write};

use tempfile::TempPath;

use crate::filters::{Filters, Flush};
use crate::input::InputStream;
use crate::io_policy::{CHUNK, IoPolicy};

pub enum Backing {
    Gzip(Box<crate::gzip::Gzip>),
    File(BufReader<File>),
    Memory(Cursor<Vec<u8>>),
    Input(InputStream),
    Output,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wrapper {
    File,
    Gzip,
    GzipDirect,
    Standard,
    Process,
    ProcessSocket,
    Memory,
    Temp,
    Data,
    Input,
    Output,
}

impl Wrapper {
    pub fn is_gzip(self) -> bool {
        matches!(self, Self::Gzip | Self::GzipDirect)
    }
}

pub struct Stream {
    pub backing: Backing,
    pub attachment: Option<Box<dyn crate::child_pipe::CloseAttachment>>,
    pub child_pipe: Option<crate::child_pipe::ChildPipe>,
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
    pub(crate) filters: Option<Box<Filters>>,
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
            attachment: None,
            child_pipe: None,
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
            filters: None,
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

    pub fn lock_descriptor(&self) -> Option<&File> {
        match &self.backing {
            Backing::Gzip(gzip) => gzip.file(),
            _ => self.descriptor(),
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
        if let Some(filters) = &self.filters
            && (filters.read.active() || !filters.buffered.is_empty())
        {
            return filters.buffered.len();
        }
        match &self.backing {
            Backing::File(file) => file.buffer().len(),
            Backing::Gzip(gzip) => gzip.unread(),
            _ => 0,
        }
    }

    pub fn read_policy(&self, limit: Option<u64>) -> IoPolicy {
        if matches!(&self.backing, Backing::Gzip(_)) {
            return IoPolicy::Blocking;
        }
        let length = limit.or_else(|| {
            self.metadata()
                .ok()
                .map(|meta| meta.len())
                .or_else(|| self.memory_size().map(|size| size as u64))
                .or_else(|| match &self.backing {
                    Backing::Input(input) => Some(input.size_hint() as u64),
                    _ => None,
                })
                .map(|length| length.saturating_sub(self.position))
        });
        IoPolicy::transfer(self.seekable && self.wrapper != Wrapper::Standard, length)
    }

    pub fn write_policy(&self, length: u64) -> IoPolicy {
        if matches!(&self.backing, Backing::Gzip(_)) {
            return IoPolicy::Blocking;
        }
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
        if let Some(mut filters) = self.filters.take() {
            let result = if filters.read.active() || !filters.buffered.is_empty() {
                self.read_filtered(&mut filters, buffer)
            } else {
                self.read_raw(buffer)
            };
            self.filters = Some(filters);
            return result;
        }
        self.read_raw(buffer)
    }

    fn read_filtered(&mut self, filters: &mut Filters, result: &mut [u8]) -> io::Result<usize> {
        let mut block = [0; CHUNK];
        let wanted = self.chunk_size.min(block.len());
        while filters.buffered.len() < result.len().min(wanted) && !filters.read_closed {
            let count = self.read_raw(&mut block[..wanted])?;
            let flush = if count == 0 {
                Flush::Closing
            } else {
                Flush::No
            };
            let bytes = filters
                .read
                .run(block[..count].to_vec(), flush, self.position)?;
            filters.read_closed = count == 0;
            if let Some(offset) = filters.read.take_seek()
                && self.seek(SeekFrom::Start(offset)).is_ok()
            {
                filters.buffered.clear();
            }
            crate::filters::bounded(filters.buffered.len() + bytes.len())?;
            filters.buffered.extend(bytes);
        }
        let count = result.len().min(filters.buffered.len());
        for (output, byte) in result.iter_mut().zip(filters.buffered.drain(..count)) {
            *output = byte;
        }
        Ok(count)
    }

    fn read_raw(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.backing {
            Backing::File(file) if !self.read_buffered && file.buffer().is_empty() => {
                file.get_mut().read(buffer)
            }
            Backing::File(file) => file.read(buffer),
            Backing::Gzip(gzip) => gzip.read(buffer),
            Backing::Memory(cursor) => cursor.read(buffer),
            Backing::Input(input) => Ok(input.read(buffer, self.chunk_size)),
            Backing::Output => Ok(0),
        }
    }

    pub fn line(&mut self, limit: Option<u64>) -> io::Result<Vec<u8>> {
        let mut line = Vec::new();
        let limit = limit.unwrap_or(u64::MAX);
        if self
            .filters
            .as_ref()
            .is_some_and(|filters| filters.read.active() || !filters.buffered.is_empty())
        {
            let mut byte = [0];
            while (line.len() as u64) < limit {
                if self.read_bytes(&mut byte)? == 0 {
                    self.eof = true;
                    break;
                }
                line.push(byte[0]);
                self.position += 1;
                if byte[0] == b'\n' {
                    break;
                }
            }
            return Ok(line);
        }
        match &mut self.backing {
            Backing::File(file) => file.take(limit).read_until(b'\n', &mut line)?,
            Backing::Gzip(gzip) => {
                let count = gzip.line(limit, &mut line)?;
                if count != 0 && gzip.at_end() {
                    self.eof = true;
                }
                count
            }
            Backing::Memory(cursor) => cursor.take(limit).read_until(b'\n', &mut line)?,
            Backing::Input(input) => {
                line = input.line(
                    usize::try_from(limit).unwrap_or(usize::MAX),
                    self.chunk_size,
                );
                line.len()
            }
            Backing::Output => 0,
        };
        self.position += line.len() as u64;
        if line.last() != Some(&b'\n') && (line.len() as u64) < limit {
            self.eof = true;
        }
        Ok(line)
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .filters
            .as_ref()
            .is_some_and(|filters| filters.write.active())
        {
            self.filter_write(bytes, Flush::No)?;
            return Ok(bytes.len());
        }
        self.write_raw(bytes)
    }

    pub(crate) fn filter_write(&mut self, bytes: &[u8], flush: Flush) -> io::Result<Vec<u8>> {
        let Some(mut filters) = self.filters.take() else {
            return Ok(Vec::new());
        };
        let result = (|| {
            let mut output = Vec::new();
            for chunk in bytes.chunks(CHUNK) {
                let transformed = filters
                    .write
                    .run(chunk.to_vec(), Flush::No, self.position)?;
                self.write_filter_output(&transformed)?;
                if self.wrapper == Wrapper::Output {
                    crate::filters::bounded(output.len() + transformed.len())?;
                    output.extend(transformed);
                }
            }
            if flush != Flush::No && filters.write.active() {
                let transformed = filters.write.run(Vec::new(), flush, self.position)?;
                if let Some(offset) = filters.write.take_seek() {
                    let _ = self.seek(SeekFrom::Start(offset));
                }
                self.write_filter_output(&transformed)?;
                if self.wrapper == Wrapper::Output {
                    output.extend(transformed);
                }
            }
            Ok(output)
        })();
        self.filters = Some(filters);
        result
    }

    pub(crate) fn write_filter_output(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            match self.write_raw(bytes) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(written) => bytes = &bytes[written..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn write_raw(&mut self, bytes: &[u8]) -> io::Result<usize> {
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
            Backing::Gzip(gzip) => gzip.write(bytes)?,
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

    pub fn seek_policy(&self) -> IoPolicy {
        if self.wrapper.is_gzip() {
            IoPolicy::Blocking
        } else {
            IoPolicy::Inline
        }
    }

    pub fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if !self.seekable {
            return Err(io::ErrorKind::Unsupported.into());
        }
        let position = match &mut self.backing {
            Backing::File(file) => file.seek(position)?,
            Backing::Gzip(gzip) => gzip.seek(self.position, position)?,
            Backing::Memory(cursor) => cursor.seek(position)?,
            Backing::Input(cursor) => cursor.seek(position)?,
            Backing::Output => return Err(io::ErrorKind::Unsupported.into()),
        };
        self.position = position;
        self.eof = false;
        if let Some(filters) = &mut self.filters {
            filters.buffered.clear();
            filters.read_closed = false;
        }
        Ok(position)
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if self
            .filters
            .as_ref()
            .is_some_and(|filters| filters.write.active())
        {
            self.filter_write(&[], Flush::Closing)?;
        }
        match &mut self.backing {
            Backing::Gzip(gzip) => gzip.finish(),
            _ => Ok(()),
        }
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
