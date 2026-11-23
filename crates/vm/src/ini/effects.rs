//! A write updates runtime fields only after PHP's setting-specific checks.
//!
//! ```php
//! ini_set('error_reporting', '0');
//! echo error_reporting(); // 0
//! ini_restore('error_reporting');
//! ```

use phera_extension_api::{NativeError, ini::IniDefinition};

use super::{Vm, boolean, integer, parse_quantity};

impl Vm {
    pub(super) fn ini_apply(
        &mut self,
        function: &str,
        entry: &IniDefinition,
        value: Option<&[u8]>,
    ) -> Result<bool, NativeError> {
        let bytes = value.unwrap_or_default();
        match entry.name {
            "include_path" | "arg_separator.output" if bytes.is_empty() => return Ok(false),
            "default_charset" | "default_mimetype" | "internal_encoding" | "input_encoding"
            | "output_encoding"
                if bytes.iter().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) =>
            {
                return Ok(false);
            }
            "error_reporting" => {
                self.error_handlers.reporting =
                    value.map_or(30719, |text| i64::from(integer(text) as i32));
                self.ini_overrides.reporting_number = None;
                self.ini_overrides.reporting_ini_modified = false;
            }
            "fatal_error_backtraces" => self.error_handlers.fatal_error_backtraces = boolean(bytes),
            "max_execution_time" => self.execution_time.restart(integer(bytes)),
            "opcache.validate_timestamps"
            | "opcache.revalidate_freq"
            | "opcache.revalidate_path" => {
                self.include_state.policy.set_ini(entry.name, bytes);
            }
            "zend.exception_ignore_args" => {
                self.error_handlers.trace_ignore_args = boolean(bytes);
            }
            "zend.exception_string_param_max_len" => {
                let Ok(limit) = usize::try_from(integer(bytes)) else {
                    return Ok(false);
                };
                if limit > 1_000_000 {
                    return Ok(false);
                }
                self.error_handlers.trace_string_limit = limit;
            }
            "precision" => {
                let precision = integer(bytes);
                if precision < -1 {
                    return Ok(false);
                }
                self.ini_overrides.precision = Some(precision as i32);
            }
            "serialize_precision" => {
                let precision = integer(bytes);
                if precision < -1 {
                    return Ok(false);
                }
                self.ini_overrides.serialization_precision = Some(precision as i32);
            }
            "zend.assertions" => {
                let assertions = self.ini_number(entry.name, bytes);
                let previous = self.ini_overrides.assertions.unwrap_or(1);
                if function != "startup" && (assertions < 0) != (previous < 0) {
                    self.warn(
                        "zend.assertions may be completely enabled or disabled only in php.ini"
                            .into(),
                    );
                    return Ok(false);
                }
                self.ini_overrides.assertions = Some(assertions);
            }
            "unserialize_max_depth" | "memory_limit" | "fiber.stack_size" => {
                if !self.ini_quantity(entry.name, bytes) {
                    return Ok(false);
                }
            }
            "auto_detect_line_endings" if boolean(bytes) => {
                self.diagnostic(
                    "Deprecated",
                    "auto_detect_line_endings is deprecated".into(),
                );
            }
            "assert.active" | "assert.exception" | "assert.warning" if !boolean(bytes) => {
                self.ini_assert_deprecation(function, entry.name);
            }
            "assert.bail" if boolean(bytes) => self.ini_assert_deprecation(function, entry.name),
            "assert.callback" if !bytes.is_empty() => {
                self.ini_assert_deprecation(function, entry.name);
            }
            _ => {}
        }
        match self.registry.ini_hook(entry.name) {
            Some(update) => update(self, entry.name, value, function),
            None => Ok(true),
        }
    }

    fn ini_number(&mut self, name: &str, bytes: &[u8]) -> i64 {
        let (number, diagnostic) = parse_quantity(bytes);
        if let Some(message) = diagnostic {
            self.warn(format!("Invalid \"{name}\" setting. {message}"));
        }
        number
    }

    fn ini_quantity(&mut self, name: &str, bytes: &[u8]) -> bool {
        let number = self.ini_number(name, bytes);
        if name == "fiber.stack_size" && number < 0 {
            self.warn("fiber.stack_size must be a positive number".into());
            return false;
        }
        // Zend allocates its heap in 2 MiB segments. The request has no Zend heap,
        // but a value that cannot fit even the first segment is still rejected.
        if name == "memory_limit" && (0..2_097_152).contains(&number) {
            self.warn(format!(
                "Failed to set memory limit to {number} bytes (Current memory usage is 2097152 bytes)"
            ));
            return false;
        }
        true
    }

    fn ini_assert_deprecation(&mut self, function: &str, name: &str) {
        self.diagnostic(
            "Deprecated",
            format!("{function}(): {name} INI setting is deprecated"),
        );
    }
}
