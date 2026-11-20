//! Supply markup to the default handler when its specialized handler is absent.
//!
//! ```php
//! xml_set_default_handler($parser, static fn ($p, $markup) => print($markup));
//! xml_parse($parser, '<item/>', true); // <item></item>
//! ```

use super::{Emission, Handler, Parser};

impl Parser {
    pub(super) fn add_fallbacks(&self, raw: &[u8], events: &mut [Emission]) {
        let Ok(raw) = std::str::from_utf8(raw) else {
            return;
        };
        for event in events {
            event.fallback = match event.handler {
                Handler::Start => raw
                    .strip_suffix("/>")
                    .map(|tag| self.encode(&format!("{tag}>")))
                    .or_else(|| Some(self.encode(raw))),
                Handler::End => raw
                    .strip_prefix('<')
                    .and_then(|tag| {
                        tag.trim_start_matches('/')
                            .split([' ', '\t', '\r', '\n', '/', '>'])
                            .next()
                    })
                    .map(|name| self.encode(&format!("</{name}>"))),
                Handler::Text if raw.starts_with('&') => Some(self.encode(raw)),
                Handler::Text => event.data.first().cloned(),
                Handler::Pi => Some(self.encode(raw)),
                _ => None,
            };
        }
    }
}
