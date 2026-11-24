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
        if self.options.symfony_stubs
            && self.options.report_unmatched_ignores
            && crate::framework_knowledge::symfony::has_internal_polyfill(&owner.name)
        {
            self.unverified_internal_ignores
                .insert(self.file.lines.line(span.start));
        }
        if !owner.internal
            || owner.name.contains("@anonymous")
            || self.options.doctrine_stubs
                && crate::framework_knowledge::doctrine::stub_is_public(&owner.name)
        {
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
        let (kind_id, kind_name) = match owner.kind {
            ClassKind::Class => ("Class", "class"),
            ClassKind::Interface => ("Interface", "interface"),
            ClassKind::Trait => ("Trait", "trait"),
            ClassKind::Enum => ("Enum", "enum"),
        };
        let message = format!(
            "Call to {static_label}method {method}() of internal {kind_name} {}{location}.",
            owner.name,
        );
        self.diagnose(&format!("{identifier}.internal{kind_id}"), span, message);
    }
}
