//! Path patterns of `[build].include` and `[build].exclude`: `storage/**/*.php`.
//!
//! A pattern is matched against a path relative to the project root, with
//! `/` separators:
//!
//! ```text
//! config/*.php        config/app.php, not config/packages/cache.php
//! config/**/*.php     config/app.php and config/packages/cache.php
//! tests               every file under tests/
//! src/Kernel?.php     src/Kernel1.php
//! ```

/// The patterns of one configuration key.
#[derive(Debug, Clone, Default)]
pub(crate) struct GlobSet {
    patterns: Vec<String>,
}

impl GlobSet {
    pub(crate) fn new(patterns: &[String]) -> Self {
        Self {
            patterns: patterns
                .iter()
                .map(|pattern| normalize(pattern))
                .filter(|pattern| !pattern.is_empty())
                .collect(),
        }
    }

    /// Whether a pattern matches `path` itself or a directory above it.
    pub(crate) fn matches(&self, path: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| matches_path_or_parent(pattern, path))
    }
}

/// A pattern as it is matched: no leading `./`, no trailing `/`.
pub(crate) fn normalize(pattern: &str) -> String {
    let pattern = pattern.trim().replace('\\', "/");
    let pattern = pattern.strip_prefix("./").unwrap_or(&pattern);
    pattern.trim_end_matches('/').to_owned()
}

/// A pattern that names a directory covers everything below it.
pub(crate) fn matches_path_or_parent(pattern: &str, path: &str) -> bool {
    matches(pattern, path)
        || path
            .match_indices('/')
            .any(|(end, _)| matches(pattern, &path[..end]))
}

/// `*` and `?` stay inside one path component; `**` crosses components.
pub(crate) fn matches(pattern: &str, path: &str) -> bool {
    matches_bytes(pattern.as_bytes(), path.as_bytes())
}

fn matches_bytes(pattern: &[u8], path: &[u8]) -> bool {
    match pattern {
        [] => path.is_empty(),
        [b'*', b'*', rest @ ..] => {
            // `a/**/b` also matches `a/b`: the directories may be none at all.
            let without_directories = rest.strip_prefix(b"/");
            without_directories.is_some_and(|rest| matches_bytes(rest, path))
                || (0..=path.len()).any(|skipped| matches_bytes(rest, &path[skipped..]))
        }
        [b'*', rest @ ..] => {
            let component = path
                .iter()
                .position(|&byte| byte == b'/')
                .unwrap_or(path.len());
            (0..=component).any(|skipped| matches_bytes(rest, &path[skipped..]))
        }
        [b'?', rest @ ..] => match path {
            [byte, tail @ ..] => *byte != b'/' && matches_bytes(rest, tail),
            [] => false,
        },
        [literal, rest @ ..] => match path {
            [byte, tail @ ..] => byte == literal && matches_bytes(rest, tail),
            [] => false,
        },
    }
}

/// The directory a pattern can be walked from: the components before its
/// first wildcard. `config/**/*.php` starts at `config`.
pub(crate) fn literal_prefix(pattern: &str) -> &str {
    let Some(wildcard) = pattern.find(['*', '?']) else {
        return pattern;
    };
    match pattern[..wildcard].rfind('/') {
        Some(separator) => &pattern[..separator],
        None => "",
    }
}

/// The extension a pattern asks for by name: `phtml` for `views/**/*.phtml`.
pub(crate) fn literal_extension(pattern: &str) -> Option<&str> {
    let file = pattern.rsplit('/').next()?;
    let (_, extension) = file.rsplit_once('.')?;
    (!extension.is_empty() && !extension.contains(['*', '?'])).then_some(extension)
}

#[cfg(test)]
mod tests {
    use super::{GlobSet, literal_extension, literal_prefix, matches, normalize};

    #[test]
    fn a_star_stays_inside_one_directory() {
        assert!(matches("config/*.php", "config/app.php"));
        assert!(!matches("config/*.php", "config/packages/cache.php"));
        assert!(!matches("*.php", "src/a.php"));
        assert!(matches("src/Kernel?.php", "src/Kernel1.php"));
        assert!(!matches("src/a?b.php", "src/a/b.php"));
    }

    #[test]
    fn a_double_star_crosses_directories_and_may_match_none() {
        assert!(matches("config/**/*.php", "config/app.php"));
        assert!(matches("config/**/*.php", "config/packages/prod/cache.php"));
        assert!(matches("**/*.php", "a.php"));
        assert!(matches(
            "var/cache/*/**/*.php",
            "var/cache/prod/App/Kernel.php"
        ));
        assert!(matches("var/cache/*/**/*.php", "var/cache/prod/url.php"));
        assert!(!matches("var/cache/*/**/*.php", "var/cache/url.php"));
        assert!(matches("**", "anything/at/all.php"));
    }

    #[test]
    fn a_directory_pattern_covers_its_files() {
        let excluded = GlobSet::new(&["tests/".into(), "./vendor/*/docs".into()]);
        assert!(excluded.matches("tests/Unit/UserTest.php"));
        assert!(excluded.matches("tests"));
        assert!(excluded.matches("vendor/acme/docs/example.php"));
        assert!(!excluded.matches("vendor/acme/src/Client.php"));
        assert!(!excluded.matches("src/tests.php"));
    }

    #[test]
    fn empty_patterns_match_nothing() {
        let none = GlobSet::new(&[String::new(), "  ".into()]);
        assert!(!none.matches("src/a.php"));
        assert_eq!(normalize("./storage/logs/"), "storage/logs");
    }

    #[test]
    fn a_pattern_is_walked_from_its_literal_directory() {
        assert_eq!(literal_prefix("config/**/*.php"), "config");
        assert_eq!(literal_prefix("bootstrap/cache/*.php"), "bootstrap/cache");
        assert_eq!(literal_prefix("*.php"), "");
        assert_eq!(literal_prefix("bin/console"), "bin/console");
        assert_eq!(literal_extension("views/**/*.phtml"), Some("phtml"));
        assert_eq!(literal_extension("views/**"), None);
        assert_eq!(literal_extension("views/*.*"), None);
    }
}
