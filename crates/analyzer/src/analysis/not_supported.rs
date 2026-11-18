//! Resolve omitted members on built-in receivers, as in `ZipArchive::FL_RECOMPRESS`.

use phera_source::deprecations::{Deprecation, ItemKind, omitted};

use super::Analyzer;

impl Analyzer<'_> {
    pub(super) fn omitted_builtin_member(
        &self,
        kind: ItemKind,
        class: &str,
        member: &str,
    ) -> Option<&'static Deprecation> {
        let ancestors = self.index.ancestry(class);
        std::iter::once(class)
            .chain(ancestors.iter().skip(1).map(|class| class.name.as_str()))
            .filter(|class| {
                self.index
                    .builtin_classes
                    .contains(&class.to_ascii_lowercase())
            })
            .find_map(|class| omitted(kind, &format!("{class}::{member}")))
    }
}
