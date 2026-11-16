use super::*;
use std::path::{Path, PathBuf};
impl Vm {
    pub(crate) async fn include(
        &mut self,
        dst: Reg,
        path: Reg,
        once: bool,
        required: bool,
    ) -> Result<(), NativeError> {
        let mut d = CollectDiagnostics::default();
        let name = to_php_string(&self.get(path), &mut d)?.to_string_lossy();
        self.diagnostics_flush(d);
        let loader = self
            .loader
            .clone()
            .ok_or_else(|| NativeError::error("Includes need a runtime code loader"))?;
        let mut result = loader.load(PathBuf::from(&name)).await;
        if let Err(e) = &result
            && (e.contains("Fatal error:") || e.contains("Parse error:"))
        {
            self.out(e.as_bytes());
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
        if let Err(e) = &result
            && (e.contains("Fatal error:") || e.contains("Parse error:"))
        {
            self.out(e.as_bytes());
            self.status = 255;
            return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                (),
            ))));
        }
        let unit = match result {
            Ok(u) => u,
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
            if let Some(f) = unit.functions.get(*id) {
                let key = f.name.to_ascii_lowercase();
                if self.functions.contains_key(&key) {
                    return Err(NativeError::error(format!(
                        "Cannot redeclare function {}()",
                        f.name
                    )));
                }
                self.functions.insert(key, (unit.clone(), *id));
            }
        }
        let mut frame = Frame::new(unit, None, Some(dst));
        let names = frame.function().locals.clone();
        for (i, name) in names.iter().enumerate() {
            let parent = self.frames.last_mut().ok_or_else(|| {
                NativeError::error("Phera internal error: include without parent")
            })?;
            let r = if let Some(i) = parent.function().locals.iter().position(|n| n == name) {
                Reg(i as u16)
            } else if let Some(r) = parent.dynamic.get(name) {
                *r
            } else {
                let r = Reg(u16::try_from(parent.slots.len())
                    .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
                parent.slots.push(None);
                parent.dynamic.insert(name.clone(), r);
                r
            };
            if let Some(slot) = parent.slots.get_mut(r.0 as usize) {
                let value = slot.take().unwrap_or_default();
                let reference = if matches!(value, Value::Reference(_)) {
                    value
                } else {
                    Value::Reference(Rc::new(RefCell::new(value)))
                };
                *slot = Some(reference.clone());
                frame.set(Reg(i as u16), reference);
            }
        }
        self.frames.push(frame);
        Ok(())
    }
}
