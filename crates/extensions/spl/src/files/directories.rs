//! Directory cursors share an entry list but keep their position on each object.
//!
//! ```php
//! $files = new FilesystemIterator('src', FilesystemIterator::SKIP_DOTS);
//! $saved = clone $files;
//! $files->next();
//! ```

mod cursor;
mod recursive;

pub(super) use cursor::{create, current, identity, index, key, next, rewind, seek, valid};
pub(super) use recursive::{children, has_children, sub_path, sub_pathname};

use phera_ext_streams::native;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PhpString, Value, args,
};
use std::rc::Rc;

use super::{
    DIRECTORY, Data, FILESYSTEM, GLOB, Location, RECURSIVE, information, join, read, update,
};
use crate::{This, receiver};

pub(super) const SKIP_DOTS: i64 = 4096;
pub(super) const FOLLOW_LINKS: i64 = 16384;
pub(super) const CURRENT_MASK: i64 = 240;
pub(super) const KEY_MASK: i64 = 3840;
pub(super) const OTHER_MASK: i64 = 28672;
pub(super) const AS_SELF: i64 = 16;
pub(super) const AS_PATHNAME: i64 = 32;
pub(super) const KEY_FILENAME: i64 = 256;

#[derive(Clone, Copy)]
pub(super) enum Current {
    FileInfo,
    Pathname,
    Directory,
}

#[derive(Clone)]
pub(super) struct Directory {
    path: PhpString,
    entries: Rc<[PhpString]>,
    glob: Option<PhpString>,
    flags: i64,
    offset: usize,
    position: i64,
    sub_path: PhpString,
}

impl Directory {
    pub(super) fn entry(&self) -> Option<&PhpString> {
        self.entries.get(self.offset)
    }

    fn filename(&self) -> &[u8] {
        self.entry().map_or(b"", |entry| {
            if self.glob.is_some() {
                native::basename(entry.as_bytes())
            } else {
                entry.as_bytes()
            }
        })
    }

    pub(super) fn location(&self) -> Location {
        if self.glob.is_some() {
            return Location::info(self.entry().map_or(b"", PhpString::as_bytes));
        }
        Location {
            path: self.path.clone(),
            pathname: join(self.path.as_bytes(), self.filename()),
        }
    }

    fn current_mode(&self) -> Current {
        match self.flags & CURRENT_MASK {
            0 => Current::FileInfo,
            AS_PATHNAME => Current::Pathname,
            _ => Current::Directory,
        }
    }

    pub(super) fn debug(&self, props: &mut PhpArray) {
        information::private(
            props,
            DIRECTORY,
            "glob",
            self.glob.clone().map_or(Value::Bool(false), Value::String),
        );
        information::private(
            props,
            RECURSIVE,
            "subPathName",
            Value::String(self.sub_path.clone()),
        );
    }

    fn skip_dots(&mut self) {
        while self.flags & SKIP_DOTS != 0 && self.entry().is_some_and(|name| dot(name.as_bytes())) {
            self.offset += 1;
        }
    }
}

fn dot(name: &[u8]) -> bool {
    matches!(name, b"." | b"..")
}

fn get<T>(object: &Object, f: impl FnOnce(&Directory) -> T) -> Result<T, NativeError> {
    read(object, |state| match &state.data {
        Data::Directory(dir) => Some(f(dir)),
        _ => None,
    })?
    .ok_or_else(|| NativeError::error("Object not initialized"))
}

fn change<T>(object: &Object, f: impl FnOnce(&mut Directory) -> T) -> Result<T, NativeError> {
    update(object, |state| match &mut state.data {
        Data::Directory(dir) => Some(f(dir)),
        _ => None,
    })?
    .ok_or_else(|| NativeError::error("Object not initialized"))
}

pub(super) fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    open(context, this, arguments, DIRECTORY)
}

pub(super) fn filesystem(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    open(context, this, arguments, FILESYSTEM)
}

pub(super) fn recursive(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    open(context, this, arguments, RECURSIVE)
}

pub(super) fn glob(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    open(context, this, arguments, GLOB)
}

fn open<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    class: &str,
) -> LocalFuture<'a, NativeResult> {
    let name = format!("{class}::__construct");
    let basic = class == DIRECTORY;
    let glob = class == GLOB;
    let default_flags = if class == FILESYSTEM { SKIP_DOTS } else { 0 };
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let path = args::bytes(&arguments, 0);
        let argument = if glob {
            "#1 ($pattern)"
        } else {
            "#1 ($directory)"
        };
        native::check_path(&name, argument, path)?;
        if path.is_empty() {
            return Err(NativeError::value_error(format!(
                "{name}(): Argument {argument} must not be empty"
            )));
        }
        if read(object, |state| matches!(state.data, Data::Directory(_)))? {
            return Err(NativeError::error(
                "Directory object is already initialized",
            ));
        }
        let mut pattern = path.strip_prefix(b"glob://").map(ToOwned::to_owned);
        if glob && pattern.is_none() {
            pattern = Some(path.to_vec());
        }
        let entries: Vec<PhpString> = if let Some(pattern) = &pattern {
            match context
                .call(&Value::from("glob"), vec![Value::from(pattern.clone())])
                .await?
            {
                Value::Array(rows) => rows
                    .values()
                    .filter_map(|entry| match entry {
                        Value::String(entry) => Some(entry.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            }
        } else {
            native::directory_entries(context, path)
                .await?
                .map_err(|error| {
                    let prefix = format!("{name}(");
                    let suffix = format!("): Failed to open directory: {}", native::reason(&error));
                    NativeError::throw_bytes(
                        "UnexpectedValueException",
                        [prefix.as_bytes(), path, suffix.as_bytes()].concat(),
                    )
                })?
                .into_iter()
                .map(PhpString::from)
                .collect()
        };
        let glob = pattern.map(|pattern| {
            let mut uri = b"glob://".to_vec();
            uri.extend(pattern);
            PhpString::from(uri)
        });
        let len = path.len() - usize::from(path.len() > 1 && path.ends_with(b"/"));
        let flags = if basic {
            AS_SELF
        } else {
            args::int(&arguments, 1).unwrap_or(default_flags)
        };
        let mut directory = Directory {
            path: PhpString::from(&path[..len]),
            entries: Rc::from(entries),
            glob,
            flags,
            offset: 0,
            position: 0,
            sub_path: PhpString::from(""),
        };
        directory.skip_dots();
        update(object, |state| state.data = Data::Directory(directory))?;
        Ok(Value::Null)
    })
}

pub(super) fn filename(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |dir| Value::from(dir.filename().to_vec()))
}

pub(super) fn is_dot(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |dir| Value::Bool(dot(dir.filename())))
}

pub(super) fn flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |dir| Value::Int(visible_flags(dir.flags)))
}

pub(super) fn set_flags(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    change(receiver(this)?, |dir| {
        dir.flags = visible_flags(args::int(arguments, 0).unwrap_or_default());
    })?;
    Ok(Value::Null)
}

fn visible_flags(flags: i64) -> i64 {
    flags & (CURRENT_MASK | KEY_MASK | OTHER_MASK)
}

pub(super) fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    get(object, |dir| dir.entries.len() as i64)
}

pub(super) fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}
