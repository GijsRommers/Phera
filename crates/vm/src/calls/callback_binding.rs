//! Internal callbacks coerce scalar arguments weakly: `array_map(fn(int $n) => $n, ['1'])`.

use phera_extension_api::NativeError;

use crate::Vm;

/// PHP calls include their source in parameter TypeErrors; internal callback checks omit it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ArgumentOrigin {
    #[default]
    Php,
    Callback,
    Attribute,
}

/// Explicit Closure::__invoke is an internal wrapper, even when called from PHP syntax.
#[derive(Clone, Copy, Default)]
pub(crate) enum ArgumentBinding {
    #[default]
    Caller,
    /// Zend compiled an operator such as `strlen($text)`, without a call frame.
    Frameless,
    Internal,
}

impl ArgumentBinding {
    /// Closure wrappers apply internal binding even when entered from PHP.
    pub(crate) fn origin(self, caller: ArgumentOrigin) -> ArgumentOrigin {
        match self {
            Self::Caller | Self::Frameless => caller,
            Self::Internal => ArgumentOrigin::Callback,
        }
    }

    pub(crate) fn has_native_frame(self) -> bool {
        !matches!(self, Self::Frameless)
    }
}

impl ArgumentOrigin {
    /// Internal callback TypeErrors omit a PHP call-site suffix.
    pub(crate) fn location_suffix(self, file: &str, line: usize) -> String {
        match self {
            Self::Php | Self::Attribute => format!(", called in {file} on line {line}"),
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
