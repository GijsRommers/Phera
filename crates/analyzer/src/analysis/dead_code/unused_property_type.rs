//! `private ?int $id = null;` can drop `int` if the initializer is its only write.

use super::*;
use crate::types::{Compatibility, TypePolicy};

impl Analyzer<'_> {
    pub(super) fn check_property_initializer_type(
        &mut self,
        class: &str,
        item: &PropertyDeclaration,
        framework_write: bool,
    ) {
        if framework_write {
            return;
        }
        let Some(property) = self.index.property(class, &item.name) else {
            return;
        };
        let Some(assigned) = item
            .default
            .as_ref()
            .and_then(crate::constant_types::literal)
            .or_else(|| {
                (property.default == crate::index::PropertyDefault::ImplicitNull)
                    .then_some(ValueType::Null)
            })
        else {
            return;
        };
        let declared = property.effective_type(self.index);
        let ValueType::Union(arms) = &declared else {
            return;
        };
        let label = if property.modifiers.is_static {
            "Static property"
        } else {
            "Property"
        };
        if declared.accepts(&assigned, self.index, TypePolicy::Strict) != Compatibility::Accepts {
            return;
        }
        let label_type = super::super::member_checks::receiver_name(&declared);
        for arm in arms.iter() {
            if arm.accepts(&assigned, self.index, TypePolicy::Strict) == Compatibility::Rejects
                && assigned.accepts(arm, self.index, TypePolicy::Strict) == Compatibility::Rejects
            {
                let name = &item.name;
                self.diagnose_dead_code(
                    "property.unusedType",
                    item.span,
                    format!(
                        "{label} {class}::${name} ({label_type}) is never assigned {arm} \
                         so it can be removed from the property type."
                    ),
                );
            }
        }
    }
}
