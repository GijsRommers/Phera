//! `stream_isatty(STDIN)` reports whether the stream is attached to a terminal.

use std::os::fd::BorrowedFd;

use phera_extension_api::value::Resource;
use rustix::fd::AsFd;

use crate::resource::with_stream;

pub fn with_descriptor<T>(
    resource: &Resource,
    operation: impl FnOnce(BorrowedFd<'_>) -> T,
) -> Option<T> {
    with_stream(resource, |stream| {
        stream.descriptor().map(|file| operation(file.as_fd()))
    })
    .ok()
    .flatten()
}

pub fn stream_kind(resource: &Resource) -> Option<&'static str> {
    if crate::directory::is_open(resource) {
        return Some("dir");
    }
    with_stream(resource, |stream| match stream.wrapper {
        crate::stream::Wrapper::Memory => "MEMORY",
        crate::stream::Wrapper::Temp => "TEMP",
        crate::stream::Wrapper::Data => "RFC2397",
        crate::stream::Wrapper::Input => "Input",
        crate::stream::Wrapper::Output => "Output",
        _ => "STDIO",
    })
    .ok()
}
