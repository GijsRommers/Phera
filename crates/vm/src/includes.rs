//! Include resolution and sharing caller variable references.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{CollectDiagnostics, Value, to_php_string};

use crate::{Vm, frame::Frame};
use std::path::{Path, PathBuf};
impl Vm {
    pub(crate) async fn include(
        &mut self,
        dst: Reg,
        path: Reg,
        once: bool,
        required: bool,
    ) -> Result<(), NativeError> {
        let mut diagnostics = CollectDiagnostics::default();
        let name = to_php_string(&self.get(path), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
        let loader = self
            .loader
            .clone()
            .ok_or_else(|| NativeError::error("Includes need a runtime code loader"))?;
        let mut result = loader.load(PathBuf::from(&name)).await;
        if let Err(error) = &result
            && (error.contains("Fatal error:") || error.contains("Parse error:"))
        {
            self.out(error.as_bytes());
            self.status = 255;
            return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                (),
            ))));
        }
        if result.is_err() && !Path::new(&name).is_absolute() {
            let (file, _) = self.location();
            if let Some(parent) = Path::new(&file).parent() {
                result = loader.load(parent.join(&name)).await;
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
        let unit = match result {
            Ok(unit) => unit,
            Err(_) => {
                let kind = if required {
                    if once { "require_once" } else { "require" }
                } else if once {
                    "include_once"
                } else {
                    "include"
                };
                self.warn(format!(
                    "{kind}({name}): Failed to open stream: No such file or directory"
                ));
                if required {
                    return Err(NativeError::error(format!(
                        "Failed opening required '{name}' (include_path='.:')"
                    )));
                }
                self.warn(format!(
                    "{kind}(): Failed opening '{name}' for inclusion (include_path='.:')"
                ));
                self.set(dst, Value::Bool(false));
                return Ok(());
            }
        };
        if once && self.included.contains(&unit.file) {
            self.set(dst, Value::Int(1));
            return Ok(());
        }
        self.included.insert(unit.file.clone());
        for id in &unit.hoisted {
            if let Some(function) = unit.functions.get(id.index()) {
                let key = function.name.to_ascii_lowercase();
                if self.functions.contains_key(&key) {
                    return Err(NativeError::error(format!(
                        "Cannot redeclare function {}()",
                        function.name
                    )));
                }
                self.functions.insert(key, (unit.clone(), id.index()));
            }
        }
        let names: Vec<String> = unit
            .main
            .locals
            .iter()
            .filter_map(|name| unit.names.get(name.index()).cloned())
            .collect();
        let mut bindings = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let parent = self.frames.last_mut().ok_or_else(|| {
                NativeError::error("Phera internal error: include without parent")
            })?;
            let register = if let Some(i) = parent.function().locals.iter().position(|n| {
                parent
                    .unit
                    .names
                    .get(n.index())
                    .is_some_and(|value| value == name)
            }) {
                Reg(i as u16)
            } else if let Some(register) = parent.dynamic.get(name) {
                *register
            } else {
                let register = Reg(u16::try_from(parent.count)
                    .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
                self.registers.push(None);
                parent.count += 1;
                parent.dynamic.insert(name.clone(), register);
                register
            };
            if let Some(slot) = parent
                .slots_mut(&mut self.registers)
                .get_mut(register.0 as usize)
            {
                let value = slot.take().unwrap_or_default();
                let reference = if matches!(value, Value::Reference(_)) {
                    value
                } else {
                    Value::Reference(Rc::new(RefCell::new(value)))
                };
                *slot = Some(reference.clone());
                bindings.push((Reg(i as u16), reference));
            }
        }
        let cache = self.code_cache(&unit);
        let mut frame = Frame::new(cache, &mut self.registers, unit, None, Some(dst));
        for (register, value) in bindings {
            frame.set(&mut self.registers, register, value);
        }
        self.frames.push(frame);
        Ok(())
    }
}
