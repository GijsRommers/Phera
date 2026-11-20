//! `$service->run()` is restricted outside the root namespace of
//! `/** @internal */ class LibraryService` when the internal-tag feature is enabled.

use super::*;

impl Analyzer<'_> {
    pub(super) fn check_internal_method(&mut self, class: &str, method: &str, span: Span) {
        if self.level < 2 || !self.options.internal_tag {
            return;
        }
        let Some((owner, signature)) = self.index.method_owner(class, method) else {
            return;
        };
        if !owner.internal && !signature.internal || owner.name.contains("@anonymous") {
            return;
        }
        let namespace = owner.name.split_once('\\').map(|(first, _)| first);
        let current = &self.context.names.namespace;
        if current.is_empty() && self.context.class.as_deref() == Some(owner.name.as_str())
            || !current.is_empty() && current.split('\\').next() == namespace
        {
            return;
        }
        let (static_label, identifier) = if signature.modifiers.is_static {
            ("static ", "staticMethod")
        } else {
            ("", "method")
        };
        let location = namespace.map_or_else(String::new, |namespace| {
            format!(" from outside its root namespace {namespace}")
        });
        let (identifier, message) = if signature.internal {
            (
                format!("{identifier}.internal"),
                format!(
                    "Call to internal {static_label}method {}::{method}(){location}.",
                    owner.name
                ),
            )
        } else {
            let kind = match owner.kind {
                ClassKind::Class => ("Class", "class"),
                ClassKind::Interface => ("Interface", "interface"),
                ClassKind::Trait => ("Trait", "trait"),
                ClassKind::Enum => ("Enum", "enum"),
            };
            (
                format!("{identifier}.internal{}", kind.0),
                format!(
                    "Call to {static_label}method {method}() of internal {} {}{location}.",
                    kind.1, owner.name,
                ),
            )
        };
        self.diagnose(&identifier, span, message);
    }
}
