//! Compiling a PHP pattern for PCRE2, with PHP's limits.
//!
//! ```php
//! preg_match('/a+/u', 'ää');  // /u: UTF-8 subject, Unicode character classes
//! preg_match('/(/', 'a');     // false; warning "preg_match(): Compilation
//!                             // failed: missing closing parenthesis at offset 1"
//! ```
//!
//! The source PCRE2 compiles is the pattern body after `transform`, with
//! PHP's match limits put in front of it as `(*LIMIT_...)` directives.

use crate::pattern;
use crate::transform::{self, Source};
use pcre2::bytes::{CaptureLocations, Regex, RegexBuilder};
use pcre2_sys::{
    PCRE2_ERROR_BAD_SUBPATTERN_REFERENCE, PCRE2_ERROR_CLASS_RANGE_ORDER,
    PCRE2_ERROR_ESCAPE_INVALID_IN_CLASS, PCRE2_ERROR_INVALID_AFTER_PARENS_QUERY,
    PCRE2_ERROR_INVALID_HEXADECIMAL, PCRE2_ERROR_INVALID_SUBPATTERN_NAME,
    PCRE2_ERROR_QUANTIFIER_INVALID, PCRE2_ERROR_UNKNOWN_ESCAPE,
    PCRE2_ERROR_UNMATCHED_CLOSING_PARENTHESIS, PCRE2_ERROR_UNRECOGNIZED_AFTER_QUERY_P,
};
use std::cell::{Cell, OnceCell};

/// `pcre.backtrack_limit` in php.ini, at its default.
const BACKTRACK_LIMIT: u32 = 1_000_000;
/// `pcre.recursion_limit` in php.ini, at its default.
const RECURSION_LIMIT: u32 = 100_000;
/// The request's PCRE settings affect both the compiler and the cache key.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct Settings {
    backtrack: u32,
    recursion: u32,
    jit: bool,
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

impl Settings {
    pub(crate) fn read(context: &dyn phera_extension_api::Context) -> Self {
        use phera_extension_api::ini::{boolean, integer};
        let default = Self::default();
        Self {
            backtrack: context
                .ini_get(b"pcre.backtrack_limit")
                .map_or(default.backtrack, |value| integer(value.as_bytes()) as u32),
            recursion: context
                .ini_get(b"pcre.recursion_limit")
                .map_or(default.recursion, |value| integer(value.as_bytes()) as u32),
            jit: context
                .ini_get(b"pcre.jit")
                .is_none_or(|value| boolean(value.as_bytes())),
        }
    }
}

/// `PCRE_JIT_STACK_MAX_SIZE` in php-src's php_pcre.c.
const JIT_STACK_MAX_SIZE: usize = 192 * 1024;

/// Compile errors that PCRE2 10.49, which the reference PHP 8.5 links,
/// reports one byte further on than the bundled 10.46 does.
const REPORTED_ONE_BYTE_LATER: [u32; 9] = [
    PCRE2_ERROR_UNKNOWN_ESCAPE,
    PCRE2_ERROR_ESCAPE_INVALID_IN_CLASS,
    PCRE2_ERROR_CLASS_RANGE_ORDER,
    PCRE2_ERROR_QUANTIFIER_INVALID,
    PCRE2_ERROR_INVALID_AFTER_PARENS_QUERY,
    PCRE2_ERROR_UNMATCHED_CLOSING_PARENTHESIS,
    PCRE2_ERROR_UNRECOGNIZED_AFTER_QUERY_P,
    PCRE2_ERROR_INVALID_SUBPATTERN_NAME,
    PCRE2_ERROR_INVALID_HEXADECIMAL,
];

pub(crate) struct Compiled {
    pub regex: Regex,
    /// Whether the subject is UTF-8: `/u`, or `(*UTF)` in the pattern.
    pub utf: bool,
    /// The pattern for the retry after an empty match (see `scan.rs`),
    /// compiled the first time it is needed.
    retry_source: String,
    retry_regex: OnceCell<Result<Regex, pcre2::Error>>,
    builder: RegexBuilder,
    /// Match data for `regex`, kept from one call to the next. Making it
    /// allocates a match context, the capture offsets and a JIT stack of up
    /// to `JIT_STACK_MAX_SIZE`, which costs more than a short match does.
    spare_locations: Cell<Option<CaptureLocations>>,
}

impl Compiled {
    /// Match data for `regex`: the spare one, or a new one when it is in use
    /// (a callback that matches the same pattern again).
    pub(crate) fn take_locations(&self) -> CaptureLocations {
        self.spare_locations
            .take()
            .unwrap_or_else(|| self.regex.capture_locations())
    }

    pub(crate) fn put_back_locations(&self, locations: CaptureLocations) {
        self.spare_locations.set(Some(locations));
    }

    pub(crate) fn retry(&self) -> Result<&Regex, pcre2::Error> {
        self.retry_regex
            .get_or_init(|| self.builder.build(&self.retry_source))
            .as_ref()
            .map_err(Clone::clone)
    }
}

/// Compiles a pattern with its delimiters and modifiers. The error is PHP's
/// warning text, without the function name.
pub(crate) fn compile(bytes: &[u8], settings: Settings) -> Result<Compiled, String> {
    let parsed = pattern::parse(bytes)?;
    let options = &parsed.options;
    if options.utf {
        check_utf8(parsed.body)?;
    }
    let source = transform::source(parsed.body, options)?;
    let mut builder = RegexBuilder::new();
    builder
        .caseless(options.insensitive)
        .multi_line(options.multiline)
        .dotall(options.dotall)
        .extended(options.extended)
        .utf(options.utf)
        .ucp(options.utf)
        .jit_if_available(settings.jit)
        .max_jit_stack_size(Some(JIT_STACK_MAX_SIZE));
    // Directives such as (*UTF) must stay first; the limits and the inline
    // options for U, J, n and r go right after them.
    let startup = transform::startup_end(&source.text);
    let limits = limits(&source.text[..startup], settings);
    let inline = if options.inline.is_empty() {
        String::new()
    } else {
        format!("(?{})", options.inline)
    };
    let prefix = format!("{}{limits}{inline}", &source.text[..startup]);
    let body = &source.text[startup..];
    // Compiled as it is first, so that an error offset maps straight back to
    // the pattern; the /A form below adds text in the middle.
    let regex = builder.build(&format!("{prefix}{body}")).map_err(|error| {
        compile_error(
            &error,
            parsed.body,
            &source,
            startup,
            limits.len() + inline.len(),
        )
    })?;
    // /A anchors the match at the offset. The conditional keeps recursion
    // into the whole pattern, (?R), from being anchored too.
    let anchored = format!("{prefix}(?(R)|\\G)(?:{body}{})", source.closing);
    let regex = if options.anchored {
        builder
            .build(&anchored)
            .map_err(|error| error.to_string())?
    } else {
        regex
    };
    Ok(Compiled {
        regex,
        utf: options.utf || source.text[..startup].contains("(*UTF"),
        retry_source: format!("(*NOTEMPTY_ATSTART){anchored}"),
        retry_regex: OnceCell::new(),
        builder,
        spare_locations: Cell::new(None),
    })
}

/// With `/u` the pattern must be UTF-8 itself. PHP reports that with the
/// message PCRE2 gives for invalid UTF-8 in a subject, which is obtained
/// here by matching the body against an empty UTF pattern.
fn check_utf8(body: &[u8]) -> Result<(), String> {
    let Err(error) = std::str::from_utf8(body) else {
        return Ok(());
    };
    let validator = RegexBuilder::new()
        .utf(true)
        .build("")
        .map_err(|e| e.to_string())?;
    let error_message = validator.find(body).expect_err("invalid UTF-8").to_string();
    let message = error_message
        .strip_prefix("PCRE2: error matching: ")
        .unwrap_or(&error_message);
    Err(format!(
        "Compilation failed: {message} at offset {}",
        error.valid_up_to()
    ))
}

/// PHP's text for a compile error: PCRE2's message, at the offset in the
/// pattern as written. `inserted` bytes of limits and inline options stand
/// after the first `startup` bytes of the compiled source.
fn compile_error(
    error: &pcre2::Error,
    body: &[u8],
    source: &Source,
    startup: usize,
    inserted: usize,
) -> String {
    let offset = error.offset().unwrap_or_default();
    let in_source = if offset < startup {
        offset
    } else {
        offset.saturating_sub(inserted)
    };
    let original = source.offsets.get(in_source).copied().unwrap_or(body.len());
    let code = u32::try_from(error.code()).unwrap_or_default();
    let one_byte_later = REPORTED_ONE_BYTE_LATER.contains(&code)
        || (code == PCRE2_ERROR_BAD_SUBPATTERN_REFERENCE
            && body.get(original.saturating_sub(1)) == Some(&b'\\'));
    let original = original + usize::from(one_byte_later);
    // The pcre2 crate prefixes its own text, such as "PCRE2: error
    // compiling pattern at offset 1: "; PHP shows PCRE2's message alone.
    let description = error.to_string();
    let message = description
        .split_once(": ")
        .map_or(description.as_str(), |(_, s)| s);
    let message = message.split_once(": ").map_or(message, |(_, s)| s);
    format!("Compilation failed: {message} at offset {original}")
}

/// PHP's limits as directives. Directives in the pattern may lower them,
/// never raise them.
fn limits(startup: &str, settings: Settings) -> String {
    let mut match_limit = settings.backtrack;
    let mut depth_limit = settings.recursion;
    for directive in startup.split("(*").skip(1) {
        let Some((name, value)) = directive.trim_end_matches(')').split_once('=') else {
            continue;
        };
        let Ok(value) = value.parse::<u32>() else {
            continue;
        };
        match name {
            "LIMIT_MATCH" => match_limit = value.min(settings.backtrack),
            "LIMIT_DEPTH" | "LIMIT_RECURSION" => depth_limit = value.min(settings.recursion),
            _ => {}
        }
    }
    format!("(*LIMIT_MATCH={match_limit})(*LIMIT_DEPTH={depth_limit})")
}
