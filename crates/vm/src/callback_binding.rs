//! Internal callbacks coerce scalar arguments weakly: `array_map(fn(int $n) => $n, ['1'])`.

use phera_extension_api::NativeError;

use crate::Vm;

/// PHP calls include their source in parameter TypeErrors; internal callback checks omit it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ArgumentOrigin {
    #[default]
    Php,
    Callback,
}

/// Explicit Closure::__invoke is an internal wrapper, even when called from PHP syntax.
#[derive(Clone, Copy, Default)]
pub(crate) enum ArgumentBinding {
    #[default]
    Caller,
    Internal,
}

impl ArgumentBinding {
    pub(crate) fn origin(self, caller: ArgumentOrigin) -> ArgumentOrigin {
        match self {
            Self::Caller => caller,
            Self::Internal => ArgumentOrigin::Callback,
        }
    }
}

impl ArgumentOrigin {
    pub(crate) fn location_suffix(self, file: &str, line: usize) -> String {
        match self {
            Self::Php => format!(", called in {file} on line {line}"),
            Self::Callback => String::new(),
        }
    }
}

impl Vm {
    /// Only synchronous argument binding uses this origin; PHP body execution restores its caller.
    pub(crate) fn with_argument_origin<T>(
        &mut self,
        origin: ArgumentOrigin,
        bind: impl FnOnce(&mut Self) -> Result<T, NativeError>,
    ) -> Result<T, NativeError> {
        if self.argument_origin == origin {
            return bind(self);
        }
        let previous = std::mem::replace(&mut self.argument_origin, origin);
        let result = bind(self);
        self.argument_origin = previous;
        result
    }
}
