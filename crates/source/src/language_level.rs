//! PHP's language level is fixed when the project starts.
//!
//! ```php
//! echo PHP_MAJOR_VERSION, '.', PHP_MINOR_VERSION; // 8.5 or 8.6
//! ```

/// The PHP minor versions that a project can select.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LanguageLevel {
    #[default]
    Php85,
    Php86,
}

/// A change to an operation that exists at both language levels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    /// Form feed (\f) is now added to the default trimmed characters of trim(), rtrim() and ltrim().
    TrimFormFeed,
    /// preg_grep() now returns false instead of a partial array when a PCRE execution error occurs
    /// (e.g. malformed UTF-8 input with the /u modifier).
    PregGrepReturnsFalse,
    /// array_filter() now raises a ValueError when an invalid $mode argument value is passed.
    ArrayFilterMode,
    /// array_change_key_case() now raises a ValueError when an invalid $case argument value is passed.
    ArrayChangeKeyCase,
    /// getenv() and putenv() now raise a ValueError when the first argument contains NUL bytes.
    EnvironmentRejectsNul,
    /// parse_str() now raises a ValueError when the $string argument contains NUL bytes.
    ParseStrRejectsNul,
    /// setlocale() now raises a ValueError when a locale name contains NUL bytes,
    /// instead of silently truncating it.
    SetLocaleRejectsNul,
    /// Arrays are now accepted only for the $locales argument.
    SetLocaleArrayArguments,
    /// pathinfo() now raises a ValueError when an invalid $flags argument value is passed.
    PathInfoFlags,
    /// scandir() now raises a ValueError when an invalid $sorting_order argument value is passed.
    ScandirOrder,
    /// number_format() now raises a ValueError when $decimals is outside the
    /// integer range instead of silently clamping very large positive values.
    NumberFormatDecimals,
    /// sleep() now raises a ValueError when $seconds is greater than the platform
    /// limit (UINT_MAX seconds, or UINT_MAX / 1000 seconds on Windows) instead of
    /// allowing the value to overflow.
    SleepLimit,
    /// usleep() now raises a ValueError when $microseconds is greater than
    /// UINT_MAX instead of allowing the value to overflow.
    UsleepLimit,
    /// pack() and unpack() now accept the "<" and ">" endianness modifiers on
    /// the signed and unsigned integer format codes.
    PackIntegerEndianness,
    /// pack() and unpack() now accept the "<" and ">" endianness modifiers on
    /// the float and double format codes.
    PackFloatEndianness,
}

/// Availability of a native declaration, resolved when the registry is built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability {
    Since86,
    Until85,
}

impl LanguageLevel {
    pub const ALL: [Self; 2] = [Self::Php85, Self::Php86];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Php85 => "8.5",
            Self::Php86 => "8.6",
        }
    }

    pub const fn release(self) -> &'static str {
        match self {
            Self::Php85 => "8.5.12",
            Self::Php86 => "8.6.0",
        }
    }

    pub const fn version(self) -> &'static str {
        match self {
            Self::Php85 => self.release(),
            Self::Php86 => "8.6.0RC7",
        }
    }

    pub const fn extra(self) -> &'static str {
        match self {
            Self::Php85 => "",
            Self::Php86 => "RC7",
        }
    }

    pub const fn components(self) -> (i64, i64, i64) {
        match self {
            Self::Php85 => (8, 5, 12),
            Self::Php86 => (8, 6, 0),
        }
    }

    pub const fn has(self, change: Change) -> bool {
        match change {
            Change::TrimFormFeed
            | Change::PregGrepReturnsFalse
            | Change::ArrayFilterMode
            | Change::ArrayChangeKeyCase
            | Change::EnvironmentRejectsNul
            | Change::ParseStrRejectsNul
            | Change::SetLocaleRejectsNul
            | Change::SetLocaleArrayArguments
            | Change::PathInfoFlags
            | Change::ScandirOrder
            | Change::NumberFormatDecimals
            | Change::SleepLimit
            | Change::UsleepLimit
            | Change::PackIntegerEndianness
            | Change::PackFloatEndianness => matches!(self, Self::Php86),
        }
    }

    pub const fn supports(self, availability: Availability) -> bool {
        matches!(
            (self, availability),
            (Self::Php85, Availability::Until85) | (Self::Php86, Availability::Since86)
        )
    }
}

impl std::str::FromStr for LanguageLevel {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|level| level.name() == name)
            .ok_or_else(|| format!("Unknown PHP version {name:?}. Known versions are 8.5 and 8.6."))
    }
}
