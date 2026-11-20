//! File status methods report failures using the calling SPL method name.
//!
//! ```php
//! echo (new SplFileInfo('report.txt'))->getSize();
//! ```

use phera_ext_streams::native::FileStatus;
use phera_extension_api::{Context, LocalFuture, NativeResult, Object, Value};
use std::rc::Rc;

use super::location;
use crate::receiver;

fn status<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    which: FileStatus,
    name: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let path = location(receiver(this.as_ref())?)?.pathname;
        which.get_async(context, path.as_bytes(), name).await
    })
}

pub(super) fn perms(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::Perms, "SplFileInfo::getPerms")
}

pub(super) fn inode(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::Inode, "SplFileInfo::getInode")
}

pub(super) fn size(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::Size, "SplFileInfo::getSize")
}

pub(super) fn owner(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::Owner, "SplFileInfo::getOwner")
}

pub(super) fn group(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::Group, "SplFileInfo::getGroup")
}

pub(super) fn atime(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(
        context,
        this,
        FileStatus::AccessTime,
        "SplFileInfo::getATime",
    )
}

pub(super) fn mtime(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(
        context,
        this,
        FileStatus::ModificationTime,
        "SplFileInfo::getMTime",
    )
}

pub(super) fn ctime(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(
        context,
        this,
        FileStatus::ChangeTime,
        "SplFileInfo::getCTime",
    )
}

pub(super) fn file_type(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::Type, "SplFileInfo::getType")
}

pub(super) fn writable(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(
        context,
        this,
        FileStatus::Writable,
        "SplFileInfo::isWritable",
    )
}

pub(super) fn readable(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(
        context,
        this,
        FileStatus::Readable,
        "SplFileInfo::isReadable",
    )
}

pub(super) fn executable(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(
        context,
        this,
        FileStatus::Executable,
        "SplFileInfo::isExecutable",
    )
}

pub(super) fn is_file(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::IsFile, "SplFileInfo::isFile")
}

pub(super) fn is_dir(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::IsDirectory, "SplFileInfo::isDir")
}

pub(super) fn is_link(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    status(context, this, FileStatus::IsLink, "SplFileInfo::isLink")
}
