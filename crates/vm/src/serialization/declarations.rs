//! A class implementing `Serializable` without both modern hooks is deprecated.

use phera_bytecode::ClassKind;

use crate::Vm;
use crate::classes::Class;
use crate::diagnostics::SourceLocation;

impl Vm {
    pub(crate) fn deprecate_legacy_serializable(&mut self, class: &Class, file: &str, line: usize) {
        if class.kind == ClassKind::Class
            && !class.is_abstract
            && class.is_a("Serializable")
            && (Self::serialization_method(class, "__serialize").is_none()
                || Self::serialization_method(class, "__unserialize").is_none())
        {
            let previous = self.diagnostic_location.replace(SourceLocation {
                file: file.into(),
                line,
            });
            self.diagnostic(
                "Deprecated",
                format!(
                    "{} implements the Serializable interface, which is deprecated. Implement \
                 __serialize() and __unserialize() instead (or in addition, if support for old \
                 PHP versions is necessary)",
                    class.name
                ),
            );
            self.diagnostic_location = previous;
        }
    }
}
