//! PHP 8.6 advances when `$file->next()` is called before `$file->current()`.

use phera_extension_api::{Change, LanguageLevel};

#[derive(Clone, Copy)]
pub(in super::super) struct Behavior {
    pub(super) next_without_cache: bool,
    pub(super) fgets_consumes_line: bool,
    pub(super) next_at_eof: bool,
    pub(super) seek_at_eof: bool,
}

impl Behavior {
    pub(in super::super) fn for_level(level: LanguageLevel) -> Self {
        Self {
            next_without_cache: level.has(Change::SplFileObjectNextWithoutCachedLine),
            fgets_consumes_line: level.has(Change::SplFileObjectFgetsConsumesLine),
            next_at_eof: level.has(Change::SplFileObjectNextAtEof),
            seek_at_eof: level.has(Change::SplFileObjectSeekAtEof),
        }
    }
}
