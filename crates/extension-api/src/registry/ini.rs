//! Versioned native INI defaults, resolved before any PHP script runs.
//!
//! ```php
//! var_dump(ini_get('filter.max_filter_count')); // PHP 8.6 adds this setting
//! ```

use super::Registry;
use crate::Availability;

impl Registry {
    /// Registers a built-in INI default where PHP exposes the setting.
    pub fn ini_at(
        &mut self,
        availability: Availability,
        name: &'static str,
        default: &'static str,
    ) {
        if self.level.supports(availability) {
            self.ini_defaults.insert(name, default);
        } else {
            self.unavailable_ini.insert(name);
        }
    }

    /// The built-in default of a setting registered for this project.
    pub fn ini_default(&self, name: &str) -> Option<&'static str> {
        self.ini_defaults.get(name).copied()
    }

    pub fn ini_unavailable(&self, name: &str) -> bool {
        self.unavailable_ini.contains(name) && !self.ini_defaults.contains_key(name)
    }
}
