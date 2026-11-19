//! Include resolution and sharing caller variable references.

use std::path::Path;

use phera_bytecode::{IncludeKind, Reg};
use phera_extension_api::{NativeError, WorkingDirectory};
use phera_value::Value;

use crate::include_lookup::{IncludeLookup, RelativeInclude, path_spelling};
use crate::{CodeLoadError, Vm, diagnostics::SourceLocation};

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
        let spelling = path_spelling(name);
        let lookup = if Path::new(&spelling).is_absolute() {
            IncludeLookup::Absolute(&spelling)
        } else {
            let cwd = self.state.get::<WorkingDirectory>();
            if cwd.is_some_and(|cwd| cwd.path().is_none()) {
                return Ok(Err("No such file or directory".into()));
            }
            let (caller, include_path) = if name.starts_with(b"./") || name.starts_with(b"../") {
                (None, None)
            } else {
                let file = self.location().file;
                let include_path = self.include_path();
                (
                    Path::new(&file).parent().map(Path::to_path_buf),
                    Some(path_spelling(include_path.as_bytes()).into_owned()),
                )
            };
            IncludeLookup::Relative(RelativeInclude {
                name: spelling.into_owned(),
                cwd: cwd.and_then(WorkingDirectory::path).map(Path::to_path_buf),
                caller,
                include_path,
            })
        };
        let policy = self.include_state.policy;
        match loader.load_include(lookup, policy).finish().await {
            Ok(unit) => Ok(Ok(unit)),
            Err(CodeLoadError::Compile(error)) => Err(self.dynamic_compile_failure(error).await),
            Err(CodeLoadError::Unavailable(error)) => {
                Ok(Err(if error.contains("Permission denied") {
                    "Permission denied".into()
                } else {
                    "No such file or directory".into()
                }))
            }
        }
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
                    let message = format!(
                        "Cannot redeclare function {}() (previously declared in {previous_file}:{previous_line})",
                        function.name,
                    );
                    return Err(self.fatal_error(
                        64,
                        message,
                        SourceLocation {
                            file: unit.file.clone(),
                            line: function.definition_line as usize,
                        },
                    ));
                }
                let main = self.code.load(unit);
                let index = self.code.declared(main, id.index());
                self.function_table.insert(key, index);
            }
        }
        Ok(())
    }
}
