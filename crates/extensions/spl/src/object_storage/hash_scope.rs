//! A `getHash()` override may read storage, but `$anotherStorage[$object] = 1`
//! raises an error while the override is running in PHP 8.6.

use std::{cell::Cell, rc::Rc};

use phera_extension_api::{Context, NativeError};

#[derive(Default)]
struct Active(Rc<Cell<bool>>);

pub(super) struct HashScope {
    active: Rc<Cell<bool>>,
    previous: bool,
}

impl Drop for HashScope {
    fn drop(&mut self) {
        self.active.set(self.previous);
    }
}

pub(super) fn enter(context: &mut dyn Context) -> HashScope {
    let active = context.state().get_or_default::<Active>().0.clone();
    let previous = active.replace(true);
    HashScope { active, previous }
}

pub(super) fn require_mutation_allowed<const CHECK: bool>(
    context: &mut dyn Context,
) -> Result<(), NativeError> {
    if CHECK
        && context
            .state()
            .get::<Active>()
            .is_some_and(|state| state.0.get())
    {
        return Err(NativeError::error(
            "Modification of SplObjectStorage during getHash() is prohibited",
        ));
    }
    Ok(())
}
