//! Diagnostic locations follow the bad read, even when a closure is called later.
//! `$f = function () { return $this; }; $f();` reports the `$this` line.

use super::*;
use crate::{index::Location, rules};

impl Analyzer<'_> {
    pub fn report(&mut self, finding: Finding, span: Span) {
        let location = Location {
            file: self.file.path.clone(),
            line: self.file.lines.line(span.start),
        };
        self.report_location(finding, &location);
    }

    pub fn report_location(&mut self, finding: Finding, location: &Location) {
        for rule in rules::all() {
            if rule.level() <= self.level
                && let Some(message) = rule.message(&finding)
            {
                self.diagnostics.push(Diagnostic {
                    file: location.file.clone(),
                    line: location.line,
                    identifier: rule.identifier().into(),
                    message,
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
