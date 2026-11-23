//! `fopen('notes', 'c+')` opens or creates a file and preserves its contents.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Opening {
    Read,
    Truncate,
    Append,
    Exclusive,
    Keep,
}

#[derive(Clone, Copy)]
pub struct Mode {
    opening: Opening,
    update: bool,
    nonblocking: bool,
}

impl Mode {
    pub fn parse(mode: &[u8]) -> Option<Self> {
        let (first, rest) = mode.split_first()?;
        let opening = match first {
            b'r' => Opening::Read,
            b'w' => Opening::Truncate,
            b'a' => Opening::Append,
            b'x' => Opening::Exclusive,
            b'c' => Opening::Keep,
            _ => return None,
        };
        Some(Self {
            opening,
            update: rest.contains(&b'+'),
            nonblocking: rest.contains(&b'n'),
        })
    }

    pub fn readable(self) -> bool {
        self.update || self.opening == Opening::Read
    }

    pub fn writable(self) -> bool {
        self.update || self.opening != Opening::Read
    }

    pub fn appending(self) -> bool {
        self.opening == Opening::Append
    }

    pub fn blocked(self) -> bool {
        !self.nonblocking
    }

    pub fn open(self, path: &Path, mask: u32) -> io::Result<File> {
        if self.opening == Opening::Read {
            return self.options().open(path);
        }
        // A new file gets this request's mask, independently of the process mask.
        // Exclusive creation lets us apply it only to files we actually created.
        let permissions = 0o666 & !mask;
        match self.options().mode(permissions).create_new(true).open(path) {
            Ok(file) => {
                file.set_permissions(std::fs::Permissions::from_mode(permissions))?;
                Ok(file)
            }
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists
                    && self.opening != Opening::Exclusive =>
            {
                let mut options = self.options();
                options.truncate(self.opening == Opening::Truncate);
                options.mode(permissions).create(true).open(path)
            }
            Err(error) => Err(error),
        }
    }

    /// Opens the file a stream of this mode opened before, as it is now:
    /// it is never created or truncated again. A fork of a snapshot opens
    /// the files the boot held this way.
    pub fn reopen(self, path: &Path) -> io::Result<File> {
        self.options().open(path)
    }

    fn options(self) -> OpenOptions {
        let mut options = OpenOptions::new();
        options
            .read(self.readable())
            .write(self.writable())
            .append(self.appending());
        if self.nonblocking {
            options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
        }
        options
    }
}
