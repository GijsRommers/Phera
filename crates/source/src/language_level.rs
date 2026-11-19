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
    /// PHP includes the NUL byte in the error for a NUL format specifier.
    FormatIncludesNulInError,
    /// Passing an object as the $data argument to http_build_query() is now
    /// deprecated. The interpretation of object values within $data as arrays
    /// is also deprecated.
    BuildQueryObjectsDeprecated,
    /// base_convert(), bindec(), hexdec() and octdec() now raise a notice when
    /// they cannot precisely convert the given number.
    BaseConversionOverflowNotice,
    /// The SORT_LOCALE_STRING constant for the family of sort functions is now deprecated.
    SortLocaleStringDeprecated,
    /// ReflectionParameter::getDocComment()
    ReflectionParameterDocComments,
    /// The listed DateTime, DateTimeImmutable, DateTimeZone, DateInterval, and
    /// DatePeriod methods now have tentative static return types.
    DateTentativeStaticReturnTypes,
    /// DirectoryIterator::key() now returns int|string, and
    /// DirectoryIterator::current() returns string|SplFileInfo|static.
    DirectoryIteratorReturnTypes,
    /// The following SplFileObject methods are now deprecated: fgetcsv(),
    /// fputcsv(), setCsvControl(), and getCsvControl().
    SplFileObjectCsvMethodsDeprecated,
    /// SplFileObject::next() now advances the stream when no prior current()
    /// call has cached a line.
    SplFileObjectNextWithoutCachedLine,
    /// SplFileObject::fgets() no longer caches the returned line for subsequent current() calls.
    SplFileObjectFgetsConsumesLine,
    /// SplFileObject::next() past EOF no longer increments key() without bound.
    SplFileObjectNextAtEof,
    /// SplFileObject::seek() past EOF now produces the same key() value as SplTempFileObject.
    SplFileObjectSeekAtEof,
    /// SplObjectStorage::getHash() implementations may no longer mutate any
    /// SplObjectStorage instance. Attempting to do so now throws an Error.
    SplObjectStorageHashMutation,
    /// The following ArrayIterator methods are now deprecated: getFlags(),
    /// setFlags(), asort(), ksort(), uasort(), uksort(), natsort(),
    /// natcasesort(), unserialize(), and serialize().
    ArrayIteratorMethodsDeprecated,
    /// The spl_classes() function is now deprecated.
    SplClassesDeprecated,
    /// The spl_object_hash() function is now deprecated.
    SplObjectHashDeprecated,
    /// Calling ReflectionProperty::setValue() with an object that is not an
    /// instance of the class on which the property was declared is now deprecated.
    ReflectionSetValueOtherClassDeprecated,
    /// Calling ReflectionProperty::setRawValue() with an object that is not an
    /// instance of the class on which the property was declared is now deprecated.
    ReflectionSetRawValueOtherClassDeprecated,
    /// Calling ReflectionMethod::invoke() or ReflectionMethod::invokeArgs() with
    /// an object and a static method is now deprecated.
    ReflectionInvokeStaticWithObjectDeprecated,
    /// The outputs of ReflectionClassConstant::__toString() and
    /// ReflectionConstant::__toString() for constants with boolean values have been
    /// changed to use `true` and `false` rather than `1` and an empty string.
    ReflectionBooleanConstantText,
    /// Unicode data tables have been updated to Unicode 18.0.
    MbstringUnicode18,
    /// filter_var_array() return type has been narrowed from array|false|null to array|false.
    FilterVarArrayReturnType,
    /// header_register_callback() now declares true as its return type.
    HeaderRegisterCallbackReturnType,
    /// Added extra info about error location to the JSON error messages returned
    /// from json_last_error_msg() and JsonException message.
    JsonErrorLocations,
    /// Form feed (\f) is now added to the default trimmed characters of trim(), rtrim() and ltrim().
    TrimFormFeed,
    /// preg_grep() now returns false instead of a partial array when a PCRE execution error occurs
    /// (e.g. malformed UTF-8 input with the /u modifier).
    PregGrepReturnsFalse,
    /// array_intersect() with at least two arrays now converts values to strings
    /// while scanning its inputs instead of during sort comparisons.
    ArrayIntersectConvertsBeforeFiltering,
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
    /// pack() and unpack() now accept the "<" and ">" endianness modifiers on
    /// the float and double format codes.
    PackEndiannessModifiers,
    /// PHP rejects named variadic arguments to pack() before entering the native function.
    PackNamedVariadicsError,
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
            Change::FormatIncludesNulInError
            | Change::BuildQueryObjectsDeprecated
            | Change::BaseConversionOverflowNotice
            | Change::SortLocaleStringDeprecated
            | Change::ReflectionParameterDocComments
            | Change::DateTentativeStaticReturnTypes
            | Change::DirectoryIteratorReturnTypes
            | Change::SplFileObjectCsvMethodsDeprecated
            | Change::SplFileObjectNextWithoutCachedLine
            | Change::SplFileObjectFgetsConsumesLine
            | Change::SplFileObjectNextAtEof
            | Change::SplFileObjectSeekAtEof
            | Change::SplObjectStorageHashMutation
            | Change::ArrayIteratorMethodsDeprecated
            | Change::SplClassesDeprecated
            | Change::SplObjectHashDeprecated
            | Change::ReflectionSetValueOtherClassDeprecated
            | Change::ReflectionSetRawValueOtherClassDeprecated
            | Change::ReflectionInvokeStaticWithObjectDeprecated
            | Change::ReflectionBooleanConstantText
            | Change::MbstringUnicode18
            | Change::FilterVarArrayReturnType
            | Change::HeaderRegisterCallbackReturnType
            | Change::JsonErrorLocations
            | Change::TrimFormFeed
            | Change::PregGrepReturnsFalse
            | Change::ArrayIntersectConvertsBeforeFiltering
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
            | Change::PackNamedVariadicsError
            | Change::PackEndiannessModifiers => matches!(self, Self::Php86),
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
