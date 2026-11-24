//! `stdout -i 1 -c 1` copies output. Mode 2 redirects it to the DBGp client.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::protocol::{Command, NAMESPACES, Wire};

#[derive(Clone, Copy, Default)]
enum Mode {
    #[default]
    Disabled,
    Copy,
    Redirect,
}

#[derive(Clone, Copy)]
pub(crate) enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Default)]
pub(crate) struct Streams {
    out: Cell<Mode>,
    err: Cell<Mode>,
    queue: RefCell<VecDeque<(Stream, Vec<u8>)>>,
}

impl Streams {
    /// Returns whether to keep writing to the application's original stream.
    pub(crate) fn buffer(&self, stream: Stream, bytes: &[u8]) -> bool {
        let mode = self.mode(stream).get();
        if matches!(mode, Mode::Copy | Mode::Redirect) {
            self.queue.borrow_mut().push_back((stream, bytes.to_vec()));
        }
        !matches!(mode, Mode::Redirect)
    }

    pub(super) fn pending(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn mode(&self, stream: Stream) -> &Cell<Mode> {
        match stream {
            Stream::Stdout => &self.out,
            Stream::Stderr => &self.err,
        }
    }

    pub(super) fn command(&self, command: &Command, stream: Stream) -> String {
        let mode = match command.get(b'c') {
            Some("0") => Mode::Disabled,
            Some("1") => Mode::Copy,
            Some("2") => Mode::Redirect,
            _ => return command.error(3, "invalid stream mode"),
        };
        self.mode(stream).set(mode);
        command.response(r#" success="1""#, "")
    }

    pub(super) async fn flush(&self, wire: &mut Wire) -> std::io::Result<()> {
        let queue = std::mem::take(&mut *self.queue.borrow_mut());
        for (kind, bytes) in queue {
            let kind = kind.name();
            let data = STANDARD.encode(bytes);
            wire.write(&format!(
                r#"<stream {NAMESPACES} type="{kind}" encoding="base64">{data}</stream>"#
            ))
            .await?;
        }
        Ok(())
    }
}
