//! SPL file information and directory cursors share the streams extension's I/O.
//!
//! ```php
//! foreach (new FilesystemIterator('src') as $file) echo $file->getFilename();
//! ```

mod dir_registration;
mod directories;
mod file;
mod information;

use phera_extension_api::{NativeError, Object, PhpString, Registry};
use std::rc::Rc;

use crate::{read_state, update_state};

const INFO: &str = "SplFileInfo";
const FILE: &str = "SplFileObject";
const DIRECTORY: &str = "DirectoryIterator";
const FILESYSTEM: &str = "FilesystemIterator";
const RECURSIVE: &str = "RecursiveDirectoryIterator";
const GLOB: &str = "GlobIterator";

pub(crate) fn register(registry: &mut Registry) {
    information::register(registry);
    dir_registration::register(registry);
    file::register(registry);
}

#[derive(Clone)]
struct Classes {
    info: Rc<str>,
    file: Rc<str>,
}

impl Default for Classes {
    fn default() -> Self {
        Self {
            info: INFO.into(),
            file: FILE.into(),
        }
    }
}

#[derive(Clone, Default)]
struct Location {
    path: PhpString,
    pathname: PhpString,
}

impl Location {
    fn info(name: &[u8]) -> Self {
        let length = name
            .iter()
            .rposition(|byte| *byte != b'/')
            .map_or(name.len().min(1), |index| index + 1);
        let boundary = name[..length]
            .iter()
            .rposition(|byte| *byte == b'/')
            .unwrap_or(0);
        Self {
            path: PhpString::from(&name[..boundary]),
            pathname: PhpString::from(&name[..length]),
        }
    }

    fn filename(&self) -> &[u8] {
        let name = self.pathname.as_bytes();
        let prefix = self.path.as_bytes().len();
        if prefix > 0 && prefix < name.len() {
            &name[prefix + 1..]
        } else {
            name
        }
    }
}

#[derive(Clone, Default)]
struct State {
    classes: Classes,
    data: Data,
}

#[derive(Clone, Default)]
enum Data {
    #[default]
    Uninitialized,
    Info(Location),
    Directory(directories::Directory),
    File(Box<file::File>),
}

impl State {
    fn location(&self) -> Result<Location, NativeError> {
        match &self.data {
            Data::Info(location) => Ok(location.clone()),
            Data::Directory(directory) => Ok(directory.location()),
            Data::File(file) => Ok(file.location.clone()),
            Data::Uninitialized => Err(NativeError::error("Object not initialized")),
        }
    }
}

fn read<T>(object: &Object, get: impl FnOnce(&State) -> T) -> Result<T, NativeError> {
    read_state(object, get)
}

fn update<T>(object: &Object, update: impl FnOnce(&mut State) -> T) -> Result<T, NativeError> {
    update_state(object, update)
}

fn join(path: &[u8], name: &[u8]) -> PhpString {
    if path.is_empty() {
        return PhpString::from(name);
    }
    let mut result = path.to_vec();
    result.push(b'/');
    result.extend_from_slice(name);
    PhpString::from(result)
}

fn clone_state(source: &Rc<Object>, destination: &Object) -> Result<(), NativeError> {
    crate::store(destination, read(source, Clone::clone)?)
}
