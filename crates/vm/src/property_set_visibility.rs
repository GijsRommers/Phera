//! `$object->id = 1` uses the property's set visibility before mutating its value.

use phera_bytecode::Visibility;
use phera_extension_api::NativeError;

use crate::{Vm, classes::class::PropertyDefault};

#[derive(Clone, Copy)]
pub(crate) enum PropertyChange {
    Direct,
    Indirect,
    Bind,
    Unset,
}

impl Vm {
    pub(crate) fn check_set_visibility(
        &self,
        property: &PropertyDefault,
        change: PropertyChange,
    ) -> Result<(), NativeError> {
        let Some(visibility) = property.set_visibility else {
            return Ok(());
        };
        let scope = self.current().and_then(|frame| frame.scope());
        let allowed = match visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == property.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                let prototype = property
                    .hooks
                    .as_ref()
                    .map_or(property.declaring.as_str(), |hooks| {
                        hooks.prototype.as_ref()
                    });
                scope.is_a(prototype)
                    || self
                        .classes
                        .get(prototype)
                        .is_some_and(|owner| owner.is_a(&scope.name))
            }),
        };
        if allowed {
            return Ok(());
        }
        let readonly = if property.is_readonly && visibility == Visibility::Protected {
            " readonly"
        } else {
            ""
        };
        let visibility = match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        };
        let action = match change {
            PropertyChange::Direct => "modify",
            PropertyChange::Indirect | PropertyChange::Bind => "indirectly modify",
            PropertyChange::Unset => "unset",
        };
        let scope = scope.map_or_else(
            || "global scope".into(),
            |scope| format!("scope {}", scope.name),
        );
        Err(NativeError::error(format!(
            "Cannot {action} {visibility}(set){readonly} property {}::${} from {scope}",
            property.declaring, property.name
        )))
    }
}
