//! `ini_set('include_path', './lib')` changes lookup only in this request.
//! The immutable registry maps names to slots. The VM holds sparse overrides
//! and the parsed fields that execution consumes without a hash lookup.
//!
//! Display and reporting settings affect diagnostics. Include paths, query
//! separators, float formatting, serialization, and exception settings affect
//! their consumers. Date, mbstring, and PCRE use the same request registry.
//! Read the generated list below for the directives without runtime consumers.
//!
//! The server sets its own request timeout and request body limits. PHP's
//! execution limit counts CPU time. No PHP configuration file is loaded.
//!
//! The session extension registers validators for its catalog entries through
//! `Registry::ini_update`. Session functions and INI calls share the same
//! values and reject writes PHP forbids while a session is active.
//!
//! Some recorded options have partial effects. The assertion guard is checked
//! after argument evaluation; bail and callback are only remembered.
//! The mbstring settings apply only to encodings and operations implemented
//! by Phera.
//! The fiber stack size validates PHP's lower bound without allocating a
//! native stack. PCRE settings use a separately compiled cache entry.

#![doc = include_str!("../../extension-api/data/ini-storage-only.md")]

use phera_extension_api::ini::{IniDefinition, boolean, integer, parse_quantity};
use phera_extension_api::{ArrayKey, NativeError, NativeResult, PhpArray, PhpString, Value};

use crate::Vm;

mod effects;

#[derive(Default, Clone)]
pub(crate) struct IniOverrides {
    startup: Vec<Option<PhpString>>,
    values: Vec<Option<PhpString>>,
    web_execution_time: bool,
    reporting_number: Option<i64>,
    reporting_ini_modified: bool,
    precision: Option<i32>,
    serialization_precision: Option<i32>,
}

impl IniOverrides {
    fn get(&self, slot: usize) -> Option<&PhpString> {
        self.values.get(slot)?.as_ref()
    }

    fn set(&mut self, slot: usize, value: PhpString) {
        if slot >= self.values.len() {
            self.values.resize(slot + 1, None);
        }
        self.values[slot] = Some(value);
    }
}

impl Vm {
    /// The web SAPI changes the default without allocating an INI override table.
    pub fn use_web_execution_time(&mut self) {
        self.ini_overrides.web_execution_time = true;
        self.execution_time.restart(30);
    }

    /// Install a SAPI setting before running PHP. Runtime writes can restore it.
    pub fn set_startup_ini(&mut self, name: &str, value: &str) {
        let Some((slot, entry)) = self.registry.ini_entry(name.as_bytes()) else {
            return;
        };
        let value = match value.to_ascii_lowercase().as_str() {
            _ if name.starts_with("xdebug.") => value,
            "on" | "yes" | "true" => "1",
            "off" | "no" | "false" | "none" | "null" => "",
            _ => value,
        };
        if !matches!(
            self.ini_apply("startup", entry, Some(value.as_bytes())),
            Ok(true)
        ) {
            return;
        }
        if self.ini_overrides.startup.len() <= slot {
            self.ini_overrides.startup.resize(slot + 1, None);
        }
        self.ini_overrides.startup[slot] = Some(value.into());
    }

    pub(crate) fn float_precision(&self) -> i32 {
        self.ini_overrides.precision.unwrap_or(14)
    }

    pub(crate) fn serialization_precision(&self) -> i32 {
        self.ini_overrides.serialization_precision.unwrap_or(-1)
    }

    pub(crate) fn ini_get(&self, name: &[u8]) -> Option<PhpString> {
        let (slot, entry) = self.registry.ini_entry(name)?;
        Some(self.ini_local(slot, entry).unwrap_or_default())
    }

    pub(crate) fn ini_value(&self, name: &[u8]) -> Option<PhpString> {
        let (slot, entry) = self.registry.ini_entry(name)?;
        self.ini_local(slot, entry)
    }

    /// Directives with a null default can be explicitly set to the empty string.
    pub(crate) fn ini_override(&self, name: &[u8]) -> Option<&PhpString> {
        let (slot, _) = self.registry.ini_entry(name)?;
        self.ini_overrides.get(slot)
    }

    fn ini_local(&self, slot: usize, entry: &IniDefinition) -> Option<PhpString> {
        if entry.name == "error_reporting"
            && let Some(level) = self.ini_overrides.reporting_number
        {
            return Some(level.to_string().into());
        }
        self.ini_overrides
            .get(slot)
            .cloned()
            .or_else(|| self.ini_default(slot, entry))
    }

    fn ini_default(&self, slot: usize, entry: &IniDefinition) -> Option<PhpString> {
        self.ini_overrides
            .startup
            .get(slot)
            .and_then(Clone::clone)
            .or_else(|| {
                if self.ini_overrides.web_execution_time && entry.name == "max_execution_time" {
                    Some("30".into())
                } else {
                    self.web_diagnostic_default(entry.name)
                        .or(entry.default.as_deref())
                        .map(PhpString::from)
                }
            })
    }

    pub(crate) fn ini_set(
        &mut self,
        function: &str,
        name: &[u8],
        value: PhpString,
    ) -> Result<Option<PhpString>, NativeError> {
        let Some((slot, entry)) = self.registry.ini_entry(name) else {
            return Ok(None);
        };
        if !entry.access.runtime() {
            return Ok(None);
        }
        let previous = self.ini_local(slot, entry).unwrap_or_default();
        if !self.ini_apply(function, entry, Some(value.as_bytes()))? {
            return Ok(None);
        }
        self.ini_overrides.set(slot, value);
        Ok(Some(previous))
    }

    pub(crate) fn ini_restore(&mut self, name: &[u8]) -> Result<(), NativeError> {
        let Some((slot, entry)) = self.registry.ini_entry(name) else {
            return Ok(());
        };
        let modified = self.ini_overrides.get(slot).is_some()
            || entry.name == "error_reporting"
                && (self.ini_overrides.reporting_number.is_some()
                    || self.ini_overrides.reporting_ini_modified);
        if !entry.access.runtime() || !modified {
            return Ok(());
        }
        let default = self.ini_default(slot, entry);
        if self.ini_apply(
            "ini_restore",
            entry,
            default.as_ref().map(PhpString::as_bytes),
        )? && let Some(value) = self.ini_overrides.values.get_mut(slot)
        {
            *value = None;
        }
        Ok(())
    }

    pub(crate) fn ini_list(&mut self, module: Option<&PhpString>, details: bool) -> NativeResult {
        let module = module.map(PhpString::as_bytes);
        if let Some(name) = module
            && !self.registry.extensions().any(|extension| {
                extension
                    .bytes()
                    .map(|b| b.to_ascii_lowercase())
                    .eq(name.iter().copied())
            })
        {
            let name = String::from_utf8_lossy(name.split(|byte| *byte == 0).next().unwrap_or(b""));
            self.warn(format!(
                "ini_get_all(): Extension \"{name}\" cannot be found"
            ));
            return Ok(Value::Bool(false));
        }
        let mut entries: Vec<_> = self
            .registry
            .ini_entries()
            .filter(|(_, entry)| {
                module.is_none_or(|name| name == b"core" || name == entry.module.as_bytes())
            })
            .collect();
        entries.sort_unstable_by_key(|(_, entry)| entry.name.to_ascii_lowercase());
        let mut list = PhpArray::new();
        for (slot, entry) in entries {
            let local = self
                .ini_local(slot, entry)
                .map_or(Value::Null, Value::String);
            let value = if details {
                let global = self
                    .ini_default(slot, entry)
                    .map_or_else(|| local.clone(), Value::String);
                let mut fields = PhpArray::new();
                fields.insert(ArrayKey::str("global_value"), global);
                fields.insert(ArrayKey::str("local_value"), local);
                fields.insert(ArrayKey::str("access"), Value::Int(entry.access as i64));
                Value::from(fields)
            } else {
                local
            };
            list.insert(ArrayKey::str(entry.name), value);
        }
        Ok(Value::from(list))
    }

    pub(crate) fn mark_reporting_ini_modified(&mut self) {
        self.ini_overrides.reporting_ini_modified = true;
    }

    /// error_reporting() keeps its number directly until an INI reader needs text.
    pub(crate) fn set_error_reporting(&mut self, level: i64) {
        self.error_handlers.reporting = i64::from(level as i32);
        self.ini_overrides.reporting_number = Some(level);
    }
}
