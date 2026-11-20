//! Streams in a fork of a snapshot.
//!
//! ```php
//! $out = fopen('php://output', 'w');   // copied: writes go to each request's output
//! $log = fopen('php://stderr', 'w');   // copied: the process's own stderr
//! $buffer = fopen('php://memory', 'r+'); fwrite($buffer, 'x');   // copied with its bytes
//! $file = fopen('/tmp/app.log', 'a');  // refused: one open file, many requests
//! Phera\snapshot();
//! ```
//!
//! A stream whose other end belongs to the process (standard output and
//! error) or to the request itself (`php://output`), or that is only bytes
//! in memory, can be given to every fork. A file or `php://input` cannot:
//! the requests would share one position and one descriptor, or read
//! another request's body.

use std::any::Any;
use std::io::{BufReader, Cursor};

use phera_extension_api::value::Fork;

use crate::stream::{Backing, Stream, Wrapper};

/// The fork of a `"stream"` resource's data.
pub(crate) fn fork_stream(data: &dyn Any, _fork: &mut Fork<'_>) -> Result<Box<dyn Any>, String> {
    let stream = data
        .downcast_ref::<Stream>()
        .ok_or("a directory handle belongs to one request")?;
    let backing = match (&stream.backing, stream.wrapper) {
        (Backing::Output, _) => Backing::Output,
        (Backing::Memory(cursor), _) if stream.temporary_path.is_none() => {
            let mut copy = Cursor::new(cursor.get_ref().clone());
            copy.set_position(cursor.position());
            Backing::Memory(copy)
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
    copy.position = stream.position;
    copy.eof = stream.eof;
    copy.blocked = stream.blocked;
    copy.read_buffered = stream.read_buffered;
    copy.chunk_size = stream.chunk_size;
    copy.spill_limit = stream.spill_limit;
    copy.data_header = stream.data_header.clone();
    Ok(Box::new(copy))
}
