//! Include resolution and sharing caller variable references.

use std::path::{Path, PathBuf};

use phera_bytecode::{IncludeKind, Reg};
use phera_extension_api::NativeError;
use phera_value::{Value, to_php_string};

use crate::{Vm, include_scope::ScopeBinding};

impl Vm {
    /// Execute included top-level code with references to the caller scope.
    pub(crate) async fn include(
        &mut self,
        destination: Reg,
        path: Reg,
        kind: IncludeKind,
    ) -> Result<(), NativeError> {
        let once = matches!(kind, IncludeKind::IncludeOnce | IncludeKind::RequireOnce);
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let name = to_php_string(&diagnostics.vm.operand(path), &mut diagnostics.buffer)?
            .to_string_lossy();
        drop(diagnostics);
        let unit = match self.load_include(&name).await? {
            Ok(unit) => unit,
            Err(reason) => return self.failed_include(destination, &name, kind, &reason),
        };
        if once && self.include_state.contains(&unit.file) {
            self.set(destination, Value::Bool(true));
            return Ok(());
        }
        if self.call_stack.depth() >= self.frame_limit {
            return Err(NativeError::error(format!(
                "Maximum call stack depth of {} exceeded",
                self.frame_limit
            )));
        }
        self.include_state.record(unit.file.clone());
        self.publish_declaration_warnings(&unit);
        self.load_unit(&unit);
        self.declare_included_functions(&unit)?;
        self.push_include_frame(unit, destination)
    }

    /// Sharing the caller scope happens before the included frame takes its register window.
    fn push_include_frame(
        &mut self,
        unit: std::sync::Arc<phera_bytecode::CompiledUnit>,
        destination: Reg,
    ) -> Result<(), NativeError> {
        let bindings = self.include_bindings(&unit)?;
        let base = self.registers.len();
        let mut frame = self.make_frame(&unit, None, Some(destination), base);
        frame.call_start = self.call_stack.preparing.len();
        for ScopeBinding {
            register,
            name,
            value,
        } in bindings
        {
            if let Some(name) = name {
                self.registers.push_undefined();
                frame.count += 1;
                self.call_stack.extras[frame.depth]
                    .dynamic
                    .insert(name, register);
            }
            frame.set(&mut self.registers, register, value);
        }
        self.call_stack.push(frame);
        Ok(())
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
                "Failed opening required '{name}' (include_path='.')"
            )));
        }
        self.warn(format!(
            "{construct}(): Failed opening '{name}' for inclusion (include_path='.')"
        ));
        self.set(destination, Value::Bool(false));
        Ok(())
    }

    /// Resolve relative includes against the caller only after the first load fails.
    /// Parse errors terminate the request before any fallback lookup.
    async fn load_include(
        &mut self,
        name: &str,
    ) -> Result<Result<std::sync::Arc<phera_bytecode::CompiledUnit>, String>, NativeError> {
        let loader = self
            .include_state
            .loader()
            .ok_or_else(|| NativeError::error("Includes need a runtime code loader"))?;
        let mut result = loader.load(PathBuf::from(name)).await;
        if let Err(error) = &result
            && (error.contains("Fatal error:") || error.contains("Parse error:"))
        {
            self.out(error.as_bytes());
            self.status = 255;
            return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                (),
            ))));
        }
        if result.is_err()
            && !Path::new(name).is_absolute()
            && !name.starts_with("./")
            && !name.starts_with("../")
        {
            let file = self.location().file;
            if let Some(parent) = Path::new(&file).parent() {
                result = loader.load(parent.join(name)).await;
            }
        }
        if let Err(error) = &result
            && (error.contains("Fatal error:") || error.contains("Parse error:"))
        {
            self.out(error.as_bytes());
            self.status = 255;
            return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                (),
            ))));
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
    fn declare_included_functions(
        &mut self,
        unit: &std::sync::Arc<phera_bytecode::CompiledUnit>,
    ) -> Result<(), NativeError> {
        for id in &unit.hoisted {
            if let Some(function) = unit.functions.get(id.index()) {
                let key = function.name.to_ascii_lowercase();
                if let Some(previous) = self.function_table.get(&key) {
                    let previous_line = previous
                        .unit
                        .functions
                        .get(previous.index)
                        .map_or(1, |function| function.definition_line);
                    let text = format!(
                        "\nFatal error: Cannot redeclare function {}() (previously declared \
                         in {}:{previous_line}) in {} on line {}\n",
                        function.name, previous.unit.file, unit.file, function.definition_line,
                    );
                    self.out(text.as_bytes());
                    self.status = 255;
                    return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                        (),
                    ))));
                }
                self.function_table.insert(
                    key,
                    crate::function_table::FunctionRef {
                        address: self.code.index(unit, Some(id.index())),
                        unit: unit.clone(),
                        index: id.index(),
                    },
                    &self.code,
                );
            }
        }
        Ok(())
    }
}
