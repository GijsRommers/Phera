//! Compile PHP patterns once: `/a+/u` uses Unicode properties and bounded PCRE2 work.

use crate::{pattern, transform};
use pcre2::bytes::{CaptureLocations, Regex, RegexBuilder};
use std::cell::{Cell, OnceCell};

pub(crate) struct Compiled {
    pub regex: Regex,
    pub utf: bool,
    retry_source: String,
    retry_regex: OnceCell<Result<Regex, pcre2::Error>>,
    builder: RegexBuilder,
    /// Match data for `regex`, kept from one call to the next. Making it
    /// allocates a match context, the capture offsets and a JIT stack of
    /// up to 192 KiB, which costs more than a short match does.
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

pub(crate) fn compile(bytes: &[u8]) -> Result<Compiled, String> {
    let parsed = pattern::parse(bytes)?;
    let options = &parsed.options;
    if options.utf
        && let Err(error) = std::str::from_utf8(parsed.body)
    {
        let validator = RegexBuilder::new()
            .utf(true)
            .build("")
            .map_err(|e| e.to_string())?;
        let error_message = validator
            .find(parsed.body)
            .expect_err("invalid UTF-8")
            .to_string();
        let message = error_message
            .strip_prefix("PCRE2: error matching: ")
            .unwrap_or(&error_message);
        return Err(format!(
            "Compilation failed: {message} at offset {}",
            error.valid_up_to()
        ));
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
        .jit_if_available(true)
        .max_jit_stack_size(Some(192 * 1024));
    let startup = transform::startup_end(&source.text);
    let limits = limits(&source.text[..startup]);
    let inline = if options.inline.is_empty() {
        String::new()
    } else {
        format!("(?{})", options.inline)
    };
    let prefix = format!("{}{limits}{inline}", &source.text[..startup]);
    let body = &source.text[startup..];
    let plain = format!("{prefix}{body}");
    // Compile without wrappers first, so invalid syntax reports the original offset.
    let regex = builder.build(&plain).map_err(|error| {
        let offset = error.offset().unwrap_or_default();
        let mapped = if offset < startup {
            offset
        } else {
            offset.saturating_sub(limits.len() + inline.len())
        };
        let original = source
            .offsets
            .get(mapped)
            .copied()
            .unwrap_or(parsed.body.len());
        // PCRE2 10.49 reports these parser errors after the offending byte.
        // The bundled 10.46 engine reports its beginning. Keep PHP 8.5 diagnostics.
        let advance = matches!(
            error.code(),
            103 | 107 | 108 | 109 | 111 | 122 | 141 | 144 | 167
        ) || (error.code() == 115
            && parsed.body.get(original.saturating_sub(1)) == Some(&b'\\'));
        let original = original + usize::from(advance);
        let description = error.to_string();
        let message = description
            .split_once(": ")
            .map_or(description.as_str(), |(_, s)| s);
        let message = message.split_once(": ").map_or(message, |(_, s)| s);
        format!("Compilation failed: {message} at offset {original}")
    })?;
    let suffix = source.closing;
    // The conditional preserves whole-pattern recursion: only the outer invocation anchors.
    let anchored = format!("{prefix}(?(R)|\\G)(?:{body}{suffix})");
    let regex = if options.anchored {
        builder
            .build(&anchored)
            .map_err(|error| error.to_string())?
    } else {
        regex
    };
    let retry_source = format!("(*NOTEMPTY_ATSTART){anchored}");
    let utf = options.utf || source.text[..startup].contains("(*UTF");
    Ok(Compiled {
        regex,
        utf,
        retry_source,
        retry_regex: OnceCell::new(),
        builder,
        spare_locations: Cell::new(None),
    })
}

/// Pattern directives may lower PHP's request limits, but must never raise them.
fn limits(startup: &str) -> String {
    let mut match_limit = 1_000_000;
    let mut depth_limit = 100_000;
    for directive in startup.split("(*").skip(1) {
        let Some((name, value)) = directive.trim_end_matches(')').split_once('=') else {
            continue;
        };
        let Ok(value) = value.parse::<u32>() else {
            continue;
        };
        match name {
            "LIMIT_MATCH" => match_limit = value.min(1_000_000),
            "LIMIT_DEPTH" | "LIMIT_RECURSION" => depth_limit = value.min(100_000),
            _ => {}
        }
    }
    format!("(*LIMIT_MATCH={match_limit})(*LIMIT_DEPTH={depth_limit})")
}
