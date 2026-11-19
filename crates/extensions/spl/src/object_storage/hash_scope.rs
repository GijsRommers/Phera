//! A `getHash()` override may read storage, but `$anotherStorage[$object] = 1`
//! raises an error while the override is running in PHP 8.6.

use phera_extension_api::{Context, NativeError};

pub(super) fn require_mutation_allowed<const CHECK: bool>(
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    if CHECK && context.state().dimension_mutation_guarded() {
        return Err(NativeError::error(
            "Modification of SplObjectStorage during getHash() is prohibited",
        ));
    }
    Ok(())
}
