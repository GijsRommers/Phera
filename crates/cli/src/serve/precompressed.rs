//! Sending `app.js.br` when the browser asked for `app.js` and accepts it.
//!
//! Build tools write compressed copies next to the files they produce:
//!
//! ```text
//! public/app.js       212 kB
//! public/app.js.br     48 kB     sent for "Accept-Encoding: gzip, br"
//! public/app.js.gz     61 kB     sent for "Accept-Encoding: gzip"
//! ```
//!
//! Nothing is compressed while a request waits: the copy either exists or
//! the file goes out as it is.

/// A compression a client may accept, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Brotli,
    Zstd,
    Gzip,
}

impl Encoding {
    /// Smallest result first. Among what the client accepts the server
    /// chooses; the client's own order of preference is not used.
    const BEST_FIRST: [Encoding; 3] = [Encoding::Brotli, Encoding::Zstd, Encoding::Gzip];

    /// The name in `Accept-Encoding` and `Content-Encoding`.
    pub(crate) fn token(self) -> &'static str {
        match self {
            Encoding::Brotli => "br",
            Encoding::Zstd => "zstd",
            Encoding::Gzip => "gzip",
        }
    }

    /// What a compressed copy's name ends in, after the original's name.
    pub(crate) fn file_suffix(self) -> &'static str {
        match self {
            Encoding::Brotli => ".br",
            Encoding::Zstd => ".zst",
            Encoding::Gzip => ".gz",
        }
    }
}

/// The encodings an `Accept-Encoding` header allows, best first.
///
/// `gzip;q=0` refuses gzip, and `*` stands for every encoding the header
/// does not name.
pub(crate) fn accepted(header: Option<&str>) -> Vec<Encoding> {
    let Some(header) = header else {
        return Vec::new();
    };
    let offers: Vec<(&str, bool)> = header.split(',').map(offer).collect();
    let wildcard = offers
        .iter()
        .find(|(token, _)| *token == "*")
        .is_some_and(|(_, wanted)| *wanted);
    Encoding::BEST_FIRST
        .into_iter()
        .filter(|encoding| {
            offers
                .iter()
                .find(|(token, _)| token.eq_ignore_ascii_case(encoding.token()))
                .map_or(wildcard, |(_, wanted)| *wanted)
        })
        .collect()
}

/// One item of the header: its name, and whether its weight is above zero.
fn offer(item: &str) -> (&str, bool) {
    let (token, parameters) = item.split_once(';').unwrap_or((item, ""));
    let weight = parameters
        .split(';')
        .find_map(|parameter| parameter.trim().strip_prefix("q="));
    // Without a weight, or with one that is not a number, the item counts.
    let wanted = weight
        .and_then(|weight| weight.trim().parse::<f32>().ok())
        .is_none_or(|weight| weight > 0.0);
    (token.trim(), wanted)
}

#[cfg(test)]
mod tests {
    use super::Encoding::{Brotli, Gzip, Zstd};
    use super::*;

    #[test]
    fn the_best_accepted_encoding_comes_first() {
        assert_eq!(
            accepted(Some("gzip, deflate, br, zstd")),
            [Brotli, Zstd, Gzip]
        );
        assert_eq!(accepted(Some("gzip")), [Gzip]);
        assert_eq!(accepted(Some("GZIP;q=0.5, BR;q=0.1")), [Brotli, Gzip]);
        assert_eq!(accepted(Some("identity")), []);
        assert_eq!(accepted(Some("")), []);
        assert_eq!(accepted(None), []);
    }

    #[test]
    fn a_weight_of_zero_refuses_and_a_star_covers_the_rest() {
        assert_eq!(accepted(Some("br;q=0, gzip")), [Gzip]);
        assert_eq!(accepted(Some("*")), [Brotli, Zstd, Gzip]);
        assert_eq!(accepted(Some("*, br;q=0")), [Zstd, Gzip]);
        assert_eq!(accepted(Some("gzip, *;q=0")), [Gzip]);
        assert_eq!(accepted(Some("gzip;q=0.0, br ; q=0.000")), []);
    }
}
