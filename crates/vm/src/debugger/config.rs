//! `XDEBUG_TRIGGER=1 phera run app.php` starts a session when debug mode is enabled.

use std::path::PathBuf;
use std::time::Duration;

/// When an enabled debugger connects to the IDE.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Always,
    Trigger,
}

/// Process settings inherited by each eligible request.
#[derive(Clone)]
pub struct DebugConfig {
    pub start: Start,
    pub boot: bool,
    pub host: String,
    pub port: u16,
    pub idekey: String,
    pub timeout: Duration,
    pub log: Option<PathBuf>,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            start: Start::Trigger,
            boot: false,
            host: "127.0.0.1".into(),
            port: 9003,
            idekey: String::new(),
            timeout: Duration::from_millis(200),
            log: None,
        }
    }
}

impl DebugConfig {
    /// Read startup settings from the file, command line, and process environment.
    pub fn from_settings(
        force: bool,
        file: &[(String, String)],
        ini: &[(String, String)],
        mode: Option<&str>,
        environment_config: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let values = file.iter().chain(ini).rev();
        let setting = |name: &str| {
            values
                .clone()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        let enabled = mode
            .or_else(|| setting("xdebug.mode"))
            .is_some_and(|modes| modes.split(',').any(|mode| mode.trim() == "debug"));
        if !force && !enabled {
            return Ok(None);
        }
        let start = match (force, setting("xdebug.start_with_request")) {
            (true, _) | (_, Some("yes")) => Start::Always,
            (_, None | Some("trigger" | "default")) => Start::Trigger,
            (_, Some("no")) => return Ok(None),
            (_, Some(_)) => return Err("Invalid xdebug.start_with_request".into()),
        };
        let mut config = Self {
            start,
            ..Self::default()
        };
        for key in [
            "client_host",
            "client_port",
            "idekey",
            "connect_timeout_ms",
            "log",
        ] {
            if let Some(value) = setting(&format!("xdebug.{key}")) {
                config.apply(key, value)?;
            }
        }
        for item in environment_config.unwrap_or_default().split_whitespace() {
            if let Some((key, value)) = item.split_once('=') {
                config.apply(key, value)?;
            }
        }
        Ok(Some(config))
    }

    /// Report effective debugger settings to PHP's INI inspection functions.
    pub fn apply_ini(&self, vm: &mut crate::Vm) {
        let start = match self.start {
            Start::Always => "yes",
            Start::Trigger => "trigger",
        };
        vm.set_startup_ini("xdebug.mode", "debug");
        vm.set_startup_ini("xdebug.start_with_request", start);
        vm.set_startup_ini("xdebug.client_host", &self.host);
        vm.set_startup_ini("xdebug.client_port", &self.port.to_string());
        vm.set_startup_ini("xdebug.idekey", &self.idekey);
        vm.set_startup_ini(
            "xdebug.connect_timeout_ms",
            &self.timeout.as_millis().to_string(),
        );
        if let Some(log) = &self.log {
            vm.set_startup_ini("xdebug.log", &log.to_string_lossy());
        }
    }

    fn apply(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "client_host" if !value.is_empty() => self.host = value.into(),
            "client_host" => return Err("xdebug.client_host is empty".into()),
            "client_port" => {
                self.port = value
                    .parse()
                    .ok()
                    .filter(|port| *port > 0)
                    .ok_or("Invalid xdebug.client_port")?;
            }
            "idekey" => self.idekey = value.into(),
            "connect_timeout_ms" => {
                let millis = value
                    .parse()
                    .map_err(|_| "Invalid xdebug.connect_timeout_ms")?;
                self.timeout = Duration::from_millis(millis);
            }
            "log" => self.log = (!value.is_empty()).then(|| PathBuf::from(value)),
            _ => {}
        }
        Ok(())
    }
}
