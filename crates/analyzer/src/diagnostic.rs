//! Stable diagnostics: `new Missing;` becomes a `class.notFound` at its source line.
//! Sorting and serialization are deterministic, independent of parsing thread order.

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub identifier: String,
    pub message: String,
}

pub fn quoted(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

impl Diagnostic {
    pub fn json(&self) -> String {
        format!(
            "{{\"file\":{},\"line\":{},\"identifier\":{},\"message\":{}}}",
            quoted(&self.file),
            self.line,
            quoted(&self.identifier),
            quoted(&self.message)
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
    Github,
}

pub fn render(diagnostics: &[Diagnostic], format: Format) -> String {
    match format {
        Format::Json => format!(
            "{{\"version\":1,\"diagnostics\":[{}]}}\n",
            diagnostics
                .iter()
                .map(Diagnostic::json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Format::Text => {
            let mut output = String::new();
            let mut file = "";
            for diagnostic in diagnostics {
                if file != diagnostic.file {
                    file = &diagnostic.file;
                    output.push_str(&format!("{file}\n"));
                }
                output.push_str(&format!(
                    "  {}  [{}] {}\n",
                    diagnostic.line, diagnostic.identifier, diagnostic.message
                ));
            }
            output
        }
        Format::Github => diagnostics
            .iter()
            .map(|d| {
                format!(
                    "::error file={},line={},title={}::{}\n",
                    annotation(&d.file, true),
                    d.line,
                    annotation(&d.identifier, true),
                    annotation(&d.message, false)
                )
            })
            .collect(),
    }
}

fn annotation(value: &str, property: bool) -> String {
    let escaped = value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    if property {
        escaped.replace(':', "%3A").replace(',', "%2C")
    } else {
        escaped
    }
}
