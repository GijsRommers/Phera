//! PHP diagnostics choose their display and log destinations independently.
//!
//! ```php
//! ini_set('display_errors', 'stderr');
//! trigger_error('look at the CLI error stream', E_USER_WARNING);
//! ```

use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;

use phera_extension_api::{RequestState, WorkingDirectory, ini};
use phera_value::PhpString;

use crate::{Vm, diagnostics::SourceLocation};

type RequestLine = fn(&RequestState) -> Option<(&str, &str, &str)>;

/// This follows the request's output when PHP switches fiber stacks.
pub(crate) struct WebDiagnostics {
    dev: bool,
    request_line: RequestLine,
}

#[derive(Clone, Copy)]
enum Display {
    Off,
    Stdout,
    Stderr,
}

impl Display {
    fn from_ini(value: Option<&PhpString>) -> Self {
        let Some(value) = value else {
            return Self::Stdout;
        };
        let bytes = value.as_bytes();
        if bytes.eq_ignore_ascii_case(b"stderr") || ini::integer(bytes) as u8 == 2 {
            Self::Stderr
        } else if bytes.eq_ignore_ascii_case(b"stdout") || ini::boolean(bytes) {
            Self::Stdout
        } else {
            Self::Off
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum DiagnosticPhase {
    RequestStartup,
    Running,
}

impl Vm {
    pub fn configure_web_diagnostics(&mut self, dev: bool, request_line: RequestLine) {
        self.output_writer.web_diagnostics = Some(WebDiagnostics { dev, request_line });
    }

    pub(super) fn web_diagnostic_default(&self, name: &str) -> Option<&'static [u8]> {
        let web = self.output_writer.web_diagnostics.as_ref()?;
        match name {
            "display_errors" => Some(if web.dev { b"1" } else { b"" }),
            "log_errors" | "html_errors" => Some(b"1"),
            _ => None,
        }
    }

    pub(super) fn ini_enabled(&self, name: &[u8]) -> bool {
        self.ini_get(name)
            .is_some_and(|value| ini::boolean(value.as_bytes()))
    }

    pub(super) fn display_errors_enabled(&self) -> bool {
        !matches!(
            Display::from_ini(self.ini_get(b"display_errors").as_ref()),
            Display::Off
        )
    }

    #[cold]
    pub(super) fn emit_diagnostic(
        &mut self,
        phase: DiagnosticPhase,
        level: i64,
        kind: &str,
        message: PhpString,
        location: SourceLocation,
    ) {
        const FATAL: i64 = 1 | 4 | 16 | 64 | 256 | 4096;
        let display = Display::from_ini(self.ini_get(b"display_errors").as_ref());
        if level & FATAL != 0 && matches!(display, Display::Off) {
            self.set_unsent_error_status();
        }
        if self.error_handlers.reporting & level != 0 || level & (16 | 32) != 0 {
            let trace = self.error_handlers.fatal_trace.as_ref().map(|trace| {
                crate::exception_rendering::render_trace(
                    trace,
                    self.error_handlers.trace_string_limit,
                )
            });
            let trace = trace.as_deref().filter(|text| !text.is_empty());
            if self.ini_enabled(b"log_errors") {
                self.log_diagnostic(kind, message.as_bytes(), &location, trace);
            }
            if !matches!(display, Display::Off)
                && (matches!(phase, DiagnosticPhase::Running)
                    || self.ini_enabled(b"display_startup_errors"))
            {
                let html = self.ini_enabled(b"html_errors");
                let cli_stderr = !html
                    && matches!(display, Display::Stderr)
                    && self.output_writer.web_diagnostics.is_none();
                let bytes = format_diagnostic(
                    kind,
                    message.as_bytes(),
                    &location,
                    level,
                    html,
                    cli_stderr,
                    trace,
                );
                if cli_stderr {
                    let _ = std::io::stderr().lock().write_all(&bytes);
                } else if let Some(bytes) = self.diagnostic_reporter.display(bytes) {
                    if matches!(phase, DiagnosticPhase::RequestStartup) {
                        self.diagnostic_location = Some(SourceLocation {
                            file: String::new(),
                            line: 0,
                        });
                        self.out(&bytes);
                        self.diagnostic_location = None;
                    } else {
                        self.out(&bytes);
                    }
                }
            }
        }
        self.record_last_error(level, message, location);
    }

    fn log_diagnostic(
        &self,
        kind: &str,
        message: &[u8],
        location: &SourceLocation,
        trace: Option<&str>,
    ) {
        let mut line = format!("PHP {kind}:  ").into_bytes();
        line.extend_from_slice(message.split(|&byte| byte == 0).next().unwrap_or_default());
        line.extend_from_slice(
            format!(" in {} on line {}", location.file, location.line).as_bytes(),
        );
        append_trace(&mut line, trace);
        if let Some(path) = self
            .ini_get(b"error_log")
            .filter(|path| !path.is_empty() && path.as_bytes() != b"syslog")
            && self
                .append_diagnostic(&path.to_string_lossy(), &line)
                .is_ok()
        {
            return;
        }
        if let Some(web) = &self.output_writer.web_diagnostics {
            if let Some((method, target, protocol)) = (web.request_line)(&self.state) {
                let mut record =
                    format!("phera-server: {method} {target} {protocol}: ").into_bytes();
                record.extend(line);
                line = record;
            }
            phera_extension_api::web_log::write(line);
        } else {
            let _ = std::io::stderr().lock().write_all(&line);
        }
    }

    fn append_diagnostic(&self, path: &str, line: &[u8]) -> std::io::Result<()> {
        let path = self.state.get::<WorkingDirectory>().map_or_else(
            || Ok(std::path::PathBuf::from(path)),
            |dir| dir.resolve(std::path::Path::new(path)),
        )?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mode = self
                .ini_get(b"error_log_mode")
                .map(|value| ini::integer(value.as_bytes()))
                .filter(|mode| (1..=0o777).contains(mode))
                .unwrap_or(0o644);
            options.mode(mode as u32);
        }
        let now = chrono::DateTime::<chrono::Utc>::from(SystemTime::now());
        let mut entry = format!("[{} UTC] ", now.format("%d-%b-%Y %H:%M:%S")).into_bytes();
        entry.extend_from_slice(line);
        options.open(path)?.write_all(&entry)
    }
}

fn format_diagnostic(
    kind: &str,
    message: &[u8],
    location: &SourceLocation,
    level: i64,
    html: bool,
    cli_stderr: bool,
    trace: Option<&str>,
) -> Vec<u8> {
    let SourceLocation { file, line } = location;
    if html {
        let mut bytes = format!("<br />\n<b>{kind}</b>:  ").into_bytes();
        if matches!(level, 1 | 4) {
            escape_html(&mut bytes, message);
        } else {
            bytes.extend_from_slice(message);
        }
        bytes
            .extend_from_slice(format!(" in <b>{file}</b> on line <b>{line}</b><br />").as_bytes());
        append_trace(&mut bytes, trace.filter(|_| !matches!(level, 1 | 4)));
        return bytes;
    }
    let newline = if cli_stderr { "" } else { "\n" };
    let mut bytes = format!("{newline}{kind}: ").into_bytes();
    bytes.extend_from_slice(message);
    bytes.extend_from_slice(format!(" in {file} on line {line}").as_bytes());
    append_trace(&mut bytes, trace);
    bytes
}

fn append_trace(bytes: &mut Vec<u8>, trace: Option<&str>) {
    if let Some(trace) = trace {
        bytes.extend_from_slice(b"\nStack trace:\n");
        bytes.extend_from_slice(trace.as_bytes());
    }
    bytes.push(b'\n');
}

fn escape_html(output: &mut Vec<u8>, message: &[u8]) {
    for &byte in message {
        let entity: &[u8] = match byte {
            b'&' => b"&amp;",
            b'<' => b"&lt;",
            b'>' => b"&gt;",
            b'"' => b"&quot;",
            _ => {
                output.push(byte);
                continue;
            }
        };
        output.extend_from_slice(entity);
    }
}
