//! Diagnostic locations follow the bad read, even when a closure is called later.
//! `$f = function () { return $this; }; $f();` reports the `$this` line.

use super::*;
use crate::{index::Location, rules};

impl Analyzer<'_> {
    pub fn diagnose(&mut self, identifier: &str, span: Span, message: String) {
        self.diagnostics.push(Diagnostic {
            severity: crate::diagnostic::Severity::Error,
            file: self.file.path.clone(),
            line: self.file.lines.line(span.start),
            identifier: identifier.into(),
            message,
            source_range: Some((span.start, span.end)),
        });
    }

    pub fn report(&mut self, finding: Finding, span: Span) {
        let location = Location {
            file: self.file.path.clone(),
            line: self.file.lines.line(span.start),
        };
        self.report_at(finding, &location, Some((span.start, span.end)));
    }

    pub fn report_location(&mut self, finding: Finding, location: &Location) {
        self.report_at(finding, location, None);
    }

    fn report_at(
        &mut self,
        finding: Finding,
        location: &Location,
        source_range: Option<(usize, usize)>,
    ) {
        for rule in rules::all() {
            if rule.level() <= self.level
                && let Some(message) = rule.message(&finding)
            {
                self.diagnostics.push(Diagnostic {
                    severity: crate::diagnostic::Severity::Error,
                    file: location.file.clone(),
                    line: location.line,
                    identifier: rule.identifier().into(),
                    message,
                    source_range,
                });
            }
        }
    }

    pub fn unbound_invocation(&mut self, signature: &crate::index::Signature) {
        if let Some(reads) = &mut self.context.unbound_this {
            reads.extend(signature.unbound_this.iter().cloned());
        } else {
            for location in &signature.unbound_this {
                self.report_location(Finding::ThisContext, location);
            }
        }
    }
}
