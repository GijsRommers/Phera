//! In `$rows[$float]`, the float is an invalid key for a PHP 8.5 array.
//! An ArrayAccess object can accept a key of its own choosing.

use super::*;

impl Analyzer<'_> {
    pub(super) fn invalid_array_offset(&mut self, array: &ValueType, key: &ValueType, span: Span) {
        if self.level < 3 || !super::operators::is_array(array) {
            return;
        }
        use ValueType as T;
        if self.strict_mixed(key) {
            self.diagnose(
                "offsetAccess.invalidOffset",
                span,
                "Possibly invalid array key type mixed.".into(),
            );
        } else if super::operators::is_array(key)
            || matches!(
                key,
                T::Float | T::Null | T::Resource | T::Class(_) | T::Generic(_) | T::Object
            )
        {
            self.diagnose(
                "offsetAccess.invalidOffset",
                span,
                format!("Invalid array key type {key}."),
            );
        }
    }
}
