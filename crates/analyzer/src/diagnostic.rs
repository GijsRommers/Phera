//! Stable diagnostics: `new Missing;` becomes a `class.notFound` at its source line.
//! Sorting and serialization are deterministic, independent of parsing thread order.

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub identifier: String,
    pub message: String,
    pub severity: Severity,
    /// Distinguishes separate expressions when they produce the same finding on one line.
    pub(crate) source_range: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    #[default]
    Error,
    Warn,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warning",
        }
    }
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

/// Preserve the version-one checker output, which consists only of errors.
pub fn render(report: &crate::Report, format: Format) -> String {
    render_report(report, format, false)
}

pub(crate) fn render_lint(report: &crate::Report, format: Format) -> String {
    render_report(report, format, true)
}

fn render_report(report: &crate::Report, format: Format, show_severity: bool) -> String {
    let diagnostics = &report.diagnostics;
    match format {
        Format::Json => format!(
            "{{\"version\":1,\"files\":{},\"vendor_files\":{},\"seconds\":{:.6},\"diagnostics\":[{}]}}\n",
            report.files,
            report.vendor_files,
            report.elapsed.as_secs_f64(),
            diagnostics
                .iter()
                .map(|diagnostic| {
                    let json = diagnostic.json();
                    if show_severity {
                        format!(
                            "{},\"severity\":\"{}\"}}",
                            json.trim_end_matches('}'),
                            diagnostic.severity.name()
                        )
                    } else {
                        json
                    }
                })
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
                    "  {}  {}[{}] {}\n",
                    diagnostic.line,
                    if show_severity && diagnostic.severity == Severity::Warn {
                        "warning "
                    } else {
                        ""
                    },
                    diagnostic.identifier,
                    diagnostic.message
                ));
            }
            output
        }
        Format::Github => diagnostics
            .iter()
            .map(|d| {
                format!(
                    "::{} file={},line={},title={}::{}\n",
                    d.severity.name(),
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
