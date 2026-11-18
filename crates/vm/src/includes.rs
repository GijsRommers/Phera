//! Include resolution and sharing caller variable references.

use std::path::Path;

use phera_bytecode::{IncludeKind, Reg};
use phera_extension_api::{NativeError, WorkingDirectory};
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Execute included top-level code with references to the caller scope.
    pub(crate) async fn include(
        &mut self,
        destination: Reg,
        path: Reg,
        kind: IncludeKind,
    ) -> Result<(), NativeError> {
        let once = matches!(kind, IncludeKind::IncludeOnce | IncludeKind::RequireOnce);
        let name = self.stringify_value(&self.get(path)).await?;
        let unit = match self.load_include(name.as_bytes()).await? {
            Ok(unit) => unit,
            Err(reason) => {
                return self.failed_include(destination, &name.to_string_lossy(), kind, &reason);
            }
        };
        if once && self.include_state.contains(&unit.file) {
            self.set(destination, Value::Bool(true));
            return Ok(());
        }
        self.include_state.record(unit.file.clone());
        self.enter_scoped_unit(unit, Some(destination))
    }
}

impl Vm {
    /// PHP emits one stream warning, then either the inclusion warning or an uncaught Error.
    fn failed_include(
        &mut self,
        destination: Reg,
        name: &str,
        kind: IncludeKind,
        reason: &str,
    ) -> Result<(), NativeError> {
        let construct = match kind {
            IncludeKind::Include => "include",
            IncludeKind::IncludeOnce => "include_once",
            IncludeKind::Require => "require",
            IncludeKind::RequireOnce => "require_once",
        };
        let display_name = Path::new(name)
            .canonicalize()
            .ok()
            .filter(|path| path.is_dir() || reason == "Permission denied")
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.to_owned());
        self.warn(format!(
            "{construct}({display_name}): Failed to open stream: {reason}"
        ));
        if matches!(kind, IncludeKind::Require | IncludeKind::RequireOnce) {
            return Err(NativeError::error(format!(
                "Failed opening required '{name}' (include_path='{}')",
                self.include_path().to_string_lossy()
            )));
        }
        self.warn(format!(
            "{construct}(): Failed opening '{name}' for inclusion (include_path='{}')",
            self.include_path().to_string_lossy()
        ));
        self.set(destination, Value::Bool(false));
        Ok(())
    }

    /// Resolve relative includes against the caller only after the first load fails.
    /// Parse errors terminate the request before any fallback lookup.
    pub(crate) async fn load_include(
        &mut self,
        name: &[u8],
    ) -> Result<Result<std::sync::Arc<phera_bytecode::CompiledUnit>, String>, NativeError> {
        let loader = self
            .include_state
            .loader()
            .ok_or_else(|| NativeError::error("Includes need a runtime code loader"))?;
        let mut candidates = Vec::new();
        let name_os = include_path_os(name);
        let path = Path::new(&name_os);
        if path.is_absolute() || name.starts_with(b"./") || name.starts_with(b"../") {
            candidates.push(path.to_path_buf());
        } else {
            let include_path = include_path_os(self.include_path().as_bytes());
            candidates.extend(std::env::split_paths(&include_path).map(|directory| {
                if directory == Path::new(".") {
                    path.to_path_buf()
                } else {
                    directory.join(path)
                }
            }));
            let file = self.location().file;
            if let Some(parent) = Path::new(&file).parent() {
                let relative = parent.join(path);
                if !candidates.contains(&relative) {
                    candidates.push(relative);
                }
            }
            if !candidates.iter().any(|candidate| candidate == path) {
                candidates.push(path.to_path_buf());
            }
        }
        let mut result = Err(String::new());
        for candidate in candidates {
            let candidate = if let Some(cwd) = self.state.get::<WorkingDirectory>() {
                match cwd.resolve(&candidate) {
                    Ok(path) => path,
                    Err(error) => {
                        result = Err(error.to_string());
                        continue;
                    }
                }
            } else {
                candidate
            };
            result = loader.load(candidate).await;
            if let Err(error) = &result
                && (error.contains("Fatal error:") || error.contains("Parse error:"))
            {
                self.out(error.as_bytes());
                self.status = 255;
                return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                    (),
                ))));
            }
            if result.is_ok() {
                break;
            }
        }
        Ok(result.map_err(|error| {
            if error.contains("Permission denied") {
                "Permission denied".into()
            } else {
                "No such file or directory".into()
            }
        }))
    }

    /// Included declarations reject names that already exist in this request.
    pub(crate) fn declare_included_functions(
        &mut self,
        unit: &std::sync::Arc<phera_bytecode::CompiledUnit>,
    ) -> Result<(), NativeError> {
        for id in &unit.hoisted {
            if let Some(function) = unit.functions.get(id.index()) {
                let key = function.name.to_ascii_lowercase();
                if let Some(previous) = self.function_table.get(&key) {
                    let previous_line = self.code.function(previous).definition_line;
                    let previous_file = &self.code.unit(previous).file;
                    let text = format!(
                        "\nFatal error: Cannot redeclare function {}() (previously declared \
                         in {}:{previous_line}) in {} on line {}\n",
                        function.name, previous_file, unit.file, function.definition_line,
                    );
                    self.out(text.as_bytes());
                    self.status = 255;
                    return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                        (),
                    ))));
                }
                let main = self.code.load(unit);
                let index = self.code.declared(main, id.index());
                self.function_table.insert(key, index);
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn include_path_os(bytes: &[u8]) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes).to_owned()
}

#[cfg(not(unix))]
fn include_path_os(bytes: &[u8]) -> std::ffi::OsString {
    String::from_utf8_lossy(bytes).into_owned().into()
}
