//! PCRE request settings, including `ini_set("pcre.backtrack_limit", "1")`.

use phera_extension_api::ini::{boolean, integer};
use phera_extension_api::{Context, Registry, copy_state};

/// `pcre.backtrack_limit` in php.ini, at its default.
const BACKTRACK_LIMIT: u32 = 1_000_000;
/// `pcre.recursion_limit` in php.ini, at its default.
const RECURSION_LIMIT: u32 = 100_000;
/// The request's PCRE settings affect both the compiler and the cache key.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct Settings {
    pub(super) backtrack: u32,
    pub(super) recursion: u32,
    pub(super) jit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backtrack: BACKTRACK_LIMIT,
            recursion: RECURSION_LIMIT,
            jit: true,
        }
    }
}

pub(super) fn register(registry: &mut Registry) {
    registry.fork_state::<Settings>(copy_state::<Settings>);
    registry.ini_update("pcre.backtrack_limit", |context, _, value, _| {
        context.state().get_or_default::<Settings>().backtrack =
            value.map_or(BACKTRACK_LIMIT, |value| integer(value) as u32);
        Ok(true)
    });
    registry.ini_update("pcre.recursion_limit", |context, _, value, _| {
        context.state().get_or_default::<Settings>().recursion =
            value.map_or(RECURSION_LIMIT, |value| integer(value) as u32);
        Ok(true)
    });
    registry.ini_update("pcre.jit", |context, _, value, _| {
        context.state().get_or_default::<Settings>().jit = value.is_none_or(boolean);
        Ok(true)
    });
}

pub(super) fn read(context: &mut dyn Context) -> Settings {
    context
        .state()
        .get::<Settings>()
        .copied()
        .unwrap_or_default()
}
