//! Normal lookup and autoload take precedence over the deprecated item policy.
//!
//! ```php
//! namespace App;
//! strftime('%Y'); // Resolves App\strftime first, then the unsupported PHP name.
//! ```

use phera_extension_api::NativeError;
use phera_source::deprecations::{self, ItemKind};

use crate::classes::{Class, class::ClassOrigin};

pub(crate) const EXCEPTION: &str = "Phera\\NotSupportedException";

#[cold]
pub(crate) fn omitted_item(kind: ItemKind, name: &str) -> Option<NativeError> {
    deprecations::omitted(kind, name)
        .map(|item| NativeError::throw(EXCEPTION, item.unsupported_message()))
}

#[cold]
pub(crate) fn omitted_member(kind: ItemKind, class: &str, member: &str) -> Option<NativeError> {
    omitted_item(kind, &format!("{class}::{member}"))
}

#[cold]
pub(crate) fn omitted_native_member(
    kind: ItemKind,
    class: &Class,
    member: &str,
) -> Option<NativeError> {
    let mut current = Some(class);
    while let Some(class) = current {
        if class.origin == ClassOrigin::Native
            && let Some(error) = omitted_member(kind, &class.name, member)
        {
            return Some(error);
        }
        current = class.parent.as_deref();
    }
    None
}

#[cold]
pub(crate) fn undefined_method(class: &Class, method: &str) -> NativeError {
    omitted_native_member(ItemKind::Method, class, method).unwrap_or_else(|| {
        NativeError::error(format!(
            "Call to undefined method {}::{method}()",
            class.name
        ))
    })
}

pub(crate) fn is_unsupported(error: &NativeError) -> bool {
    matches!(
        error,
        NativeError::Throw {
            class: EXCEPTION,
            ..
        }
    )
}
