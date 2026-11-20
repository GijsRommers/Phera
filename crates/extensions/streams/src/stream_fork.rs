//! Streams in a fork of a snapshot.
//!
//! ```php
//! $out = fopen('php://output', 'w');   // copied: writes go to each request's output
//! $log = fopen('php://stderr', 'w');   // copied: the process's own stderr
//! $buffer = fopen('php://memory', 'r+'); fwrite($buffer, 'x');   // copied with its bytes
//! $file = fopen('/tmp/app.log', 'a');  // opened again: each fork's own descriptor
//! Phera\snapshot();
//! ```
//!
//! A stream whose other end belongs to the process (standard output and
//! error) or to the request itself (`php://output`), or that is only bytes
//! in memory, can be given to every fork. A file is opened again in each
//! fork, by the path the boot opened, with the same mode but never created
//! or truncated again, at the boot's position: the requests must not share
//! one position and one descriptor. Refused: a file the boot holds a
//! `flock()` on (the snapshot keeps that lock, so a fork could not take it),
//! `php://input` (one request's body), an open gzip file, a directory
//! handle and a pipe to a process. A temporary file (`tmpfile()`, or
//! `php://temp` past its memory limit) is copied into a new one per fork.

use std::any::Any;
use std::io::{BufReader, Cursor};

use phera_extension_api::value::Fork;

use crate::stream::{Backing, Stream, Wrapper};
use crate::user_resource::UserHandle;

/// The fork of a `"stream"` resource's data.
pub(crate) fn fork_stream(data: &dyn Any, _fork: &mut Fork<'_>) -> Result<Box<dyn Any>, String> {
    if let Some(file) = data.downcast_ref::<UserHandle<crate::user_file::UserFile>>() {
        return Err(format!(
            "open user wrapper {} for {} belongs to the boot",
            file.value.object.class.name(),
            String::from_utf8_lossy(&file.value.uri)
        ));
    }
    if let Some(directory) = data.downcast_ref::<UserHandle<crate::user_directory::UserDirectory>>()
    {
        return Err(format!(
            "open user wrapper directory {} belongs to the boot",
            directory.value.object.class.name()
        ));
    }
    let stream = data
        .downcast_ref::<Stream>()
        .ok_or("a directory handle belongs to one request")?;
    if stream.filters.as_ref().is_some_and(|filters| {
        filters.read.active() || filters.write.active() || !filters.buffered.is_empty()
    }) {
        return Err(format!(
            "an attached stream filter on {} belongs to the boot",
            String::from_utf8_lossy(&stream.uri)
        ));
    }
    let mut temporary = None;
    let backing = match (&stream.backing, stream.wrapper) {
        (Backing::Output, _) => Backing::Output,
        (Backing::Memory(cursor), _) if stream.temporary_path.is_none() => {
            let mut copy = Cursor::new(cursor.get_ref().clone());
            copy.set_position(cursor.position());
            Backing::Memory(copy)
        }
        (Backing::File(file), Wrapper::Temp) => copy_temporary(file.get_ref(), stream)?.0,
        (Backing::File(file), Wrapper::File) if stream.temporary_path.is_some() => {
            let (backing, path) = copy_temporary(file.get_ref(), stream)?;
            temporary = path;
            backing
        }
        (Backing::File(_), Wrapper::File) => reopen(stream)?,
        (Backing::File(_), Wrapper::Process | Wrapper::ProcessSocket) => {
            return Err("a pipe to a process the boot started belongs to one request".into());
        }
        (Backing::File(file), Wrapper::Standard) => {
            let file = file
                .get_ref()
                .try_clone()
                .map_err(|error| format!("the standard stream could not be shared: {error}"))?;
            Backing::File(BufReader::new(file))
        }
        (Backing::Input(_), _) => return Err("php://input is the body of one request".into()),
        (Backing::Gzip(_), _) => return Err("an open gzip file belongs to one request".into()),
        _ => return Err("an open file belongs to one request".into()),
    };
    let mut copy = Stream::new(backing, stream.wrapper, &stream.mode, &stream.uri);
    copy.path = stream.path.clone();
    copy.temporary_path = temporary;
    copy.position = stream.position;
    copy.eof = stream.eof;
    copy.blocked = stream.blocked;
    copy.read_buffered = stream.read_buffered;
    copy.chunk_size = stream.chunk_size;
    copy.spill_limit = stream.spill_limit;
    copy.data_header = stream.data_header.clone();
    Ok(Box::new(copy))
}

/// The fork's own copy of a temporary file (`tmpfile()`, or `php://temp`
/// past its memory limit), at the same position: one fork's writes must not
/// reach another's. A `tmpfile()` gets a new name of its own, removed when
/// the fork's stream closes.
fn copy_temporary(
    original: &std::fs::File,
    stream: &Stream,
) -> Result<(Backing, Option<tempfile::TempPath>), String> {
    use std::io::{Seek, SeekFrom, Write};
    use std::os::unix::fs::FileExt;
    let failed = |error: std::io::Error| format!("the temporary file could not be copied: {error}");
    let (mut file, path) = if stream.temporary_path.is_some() {
        let named = tempfile::Builder::new()
            .prefix("php")
            .rand_bytes(19)
            .tempfile()
            .map_err(failed)?;
        let (file, path) = named.into_parts();
        (file, Some(path))
    } else {
        (tempfile::tempfile().map_err(failed)?, None)
    };
    let mut buffer = vec![0; crate::io_policy::CHUNK];
    let mut offset = 0;
    loop {
        let read = original.read_at(&mut buffer, offset).map_err(failed)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(failed)?;
        offset += read as u64;
    }
    file.seek(SeekFrom::Start(stream.position))
        .map_err(failed)?;
    Ok((
        Backing::File(BufReader::with_capacity(crate::io_policy::CHUNK, file)),
        path,
    ))
}

/// The fork's own descriptor of the file a boot's stream opened.
fn reopen(stream: &Stream) -> Result<Backing, String> {
    use std::io::{Seek, SeekFrom};
    if stream.locked {
        return Err(
            "the boot holds a flock() on this file; release it before Phera\\snapshot()".into(),
        );
    }
    let path = stream
        .path
        .as_ref()
        .ok_or("an open file belongs to one request")?;
    let mode = crate::mode::Mode::parse(&stream.mode).ok_or("its mode is not valid")?;
    let mut file = mode
        .reopen(path)
        .map_err(|error| format!("{} could not be opened again: {error}", path.display()))?;
    file.seek(SeekFrom::Start(stream.position))
        .map_err(|error| format!("{} could not be opened again: {error}", path.display()))?;
    Ok(Backing::File(BufReader::with_capacity(
        crate::io_policy::CHUNK,
        file,
    )))
}
