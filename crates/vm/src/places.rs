//! Paths are walked and rebuilt iteratively, applying COW at every array level.

use phera_bytecode::{PathBase, Reg, Step};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Value, to_php_string};

use crate::access_mode::{Creation, Missing};
use crate::{AccessPath, Vm};
use crate::{inline_stack::InlineStack, path_update::PathKey};

impl Vm {
    /// Resolve a dynamic name, optionally creating its register for a write.
    pub(crate) fn base_register(
        &mut self,
        place: &AccessPath,
        creation: Creation,
        missing: Missing,
    ) -> Result<Option<Reg>, NativeError> {
        let base = match place.base()? {
            PathBase::This { scratch } => {
                self.fetch_this(scratch)?;
                return Ok(Some(scratch));
            }
            PathBase::StaticProperty { scratch, .. }
            | PathBase::NamedStaticProperty { scratch, .. } => {
                let Some((class, name)) = crate::static_places::static_base(place.base()?) else {
                    return Err(internal_error!("static property base"));
                };
                let reference = match self.static_property(class, name) {
                    Ok(reference) => reference,
                    Err(error)
                        if creation == Creation::Existing
                            && missing != Missing::Warn
                            && !crate::autoload::ClassMiss::is(&error) =>
                    {
                        return Ok(None);
                    }
                    Err(error) => return Err(error),
                };
                self.set(scratch, reference);
                return Ok(Some(scratch));
            }
            PathBase::Register(register) => return Ok(Some(register)),
            PathBase::Named(register) => register,
            PathBase::GlobalNamed {
                name: register,
                scratch,
            } => {
                let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
                let result =
                    to_php_string(&diagnostics.vm.operand(register), &mut diagnostics.buffer);
                drop(diagnostics);
                let leaf = place.steps()?.next().is_none();
                return self.global_register(
                    result?.to_string_lossy(),
                    scratch,
                    creation,
                    missing,
                    leaf,
                );
            }
            PathBase::Globals => return Err(internal_error!("global table has no register")),
        };
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let name = to_php_string(&diagnostics.vm.operand(base), &mut diagnostics.buffer)?
            .to_string_lossy();
        drop(diagnostics);
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("place without frame"))?;
        if let Some(register) = frame.named_register(&name) {
            return Ok(Some(register));
        }
        if creation == Creation::Create
            || (self.in_global_scope() && self.globals.get(&name).is_some())
        {
            self.named_slot(&name).map(Some)
        } else {
            if missing == Missing::Warn {
                self.warn(format!("Undefined variable ${name}"))
            }
            Ok(None)
        }
    }

    /// Convert path keys in source order while reusing the operator diagnostic buffer.
    pub(crate) fn keys(&mut self, place: &AccessPath) -> Result<InlineStack<PathKey>, NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let mut keys = InlineStack::new();
        let result = (|| {
            for step in place.steps()? {
                let key = match step {
                    Step::Index(register) => PathKey::Index(Some(ArrayKey::from_value_diag(
                        &diagnostics.vm.operand(register),
                        &mut diagnostics.buffer,
                    )?)),
                    Step::Append => PathKey::Index(None),
                    Step::Property(name) => {
                        PathKey::Property(ArrayKey::str(diagnostics.vm.name(name)?))
                    }
                    Step::DynamicProperty(register) => PathKey::Property(ArrayKey::str(
                        to_php_string(&diagnostics.vm.operand(register), &mut diagnostics.buffer)?
                            .as_bytes(),
                    )),
                };
                keys.push(key);
            }
            Ok(())
        })();
        drop(diagnostics);
        result.map(|()| keys)
    }

    /// Read through PHP references, suppressing missing-slot warnings when requested.
    pub(crate) fn read_place(&mut self, place: &AccessPath, missing: Missing) -> NativeResult {
        if place.base()? == PathBase::Globals {
            return self.global_snapshot();
        }
        let Some(register) = self.base_register(place, Creation::Existing, missing)? else {
            return Ok(Value::Null);
        };
        self.check_static_read(place, missing)?;
        let mut value = self.read_root(register, missing);
        for step in place.steps()? {
            value = self.ordinary_step(value, step, missing)?;
        }
        Ok(value)
    }
    /// An uninitialized static slot must remain distinct from a null value.
    pub(crate) fn check_static_read(
        &mut self,
        place: &AccessPath,
        missing: Missing,
    ) -> Result<(), NativeError> {
        if let Some((class, name)) = crate::static_places::static_base(place.base()?) {
            let (class_name, name) = self.static_property_names(class, name)?;
            let states = &mut self.class_states;
            if missing == Missing::Warn
                && self.classes.get(&class_name).is_some_and(|class| {
                    class
                        .static_properties
                        .iter()
                        .filter(|property| property.name == name)
                        .any(|property| {
                            states
                                .static_property(property)
                                .is_some_and(|storage| !storage.value_initialized.get())
                        })
                })
            {
                return Err(NativeError::error(format!(
                    "Typed static property {class_name}::${name} must not be accessed before \
                     initialization"
                )));
            }
        }
        Ok(())
    }
}

pub(crate) fn display_key(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(index) => index.to_string(),
        ArrayKey::String(string) => format!("\"{}\"", string.to_string_lossy()),
    }
}

impl Vm {
    /// Warn for each path key of `instruction` that is a named local with no
    /// value yet.
    ///
    /// A key used to be a temporary the compiler filled with a `Move` from
    /// the local, and that `Move` raised PHP's "Undefined variable" warning
    /// before the path was walked. The compiler can now name the local in the
    /// path itself; reading it in place moves the warning here, once, in key
    /// order and before the base is read. Temporaries are always assigned
    /// before use, so they never warn.
    ///
    /// ```php
    /// $row[$key];   // Undefined variable $key when $key was never assigned
    /// $o->$prop;
    /// ```
    pub(crate) fn warn_undefined_path_keys(
        &mut self,
        instruction: phera_bytecode::Instruction,
    ) -> Result<(), NativeError> {
        let path = match instruction {
            phera_bytecode::Instruction::Fetch { path, .. }
            | phera_bytecode::Instruction::FetchIndex { path, .. }
            | phera_bytecode::Instruction::AssignPath { path, .. }
            | phera_bytecode::Instruction::AssignPathFromLocal { path, .. }
            | phera_bytecode::Instruction::AssignOpPath { path, .. }
            | phera_bytecode::Instruction::IncDecPath { path, .. }
            | phera_bytecode::Instruction::UnsetPath { path }
            | phera_bytecode::Instruction::IssetPath { path, .. }
            | phera_bytecode::Instruction::MakeReference { path, .. }
            | phera_bytecode::Instruction::BindReference { path, .. }
            | phera_bytecode::Instruction::SendPath { path }
            | phera_bytecode::Instruction::IterStartReference { path, .. } => path,
            _ => return Ok(()),
        };
        let place = self.path(path)?;
        for step in place.steps()? {
            let register = match step {
                Step::Index(register) | Step::DynamicProperty(register) => register,
                Step::Property(_) | Step::Append => continue,
            };
            let missing = self.current().and_then(|frame| {
                if register.index() >= frame.function().locals.len() {
                    return None;
                }
                let defined = frame
                    .slots(&self.registers)
                    .get(register.index())
                    .is_some_and(Option::is_some);
                if defined {
                    return None;
                }
                frame
                    .function()
                    .locals
                    .get(register.index())
                    .and_then(|name| frame.unit().names.get(name.index()))
                    .cloned()
            });
            if let Some(name) = missing {
                self.warn(format!("Undefined variable ${name}"));
            }
        }
        Ok(())
    }

    /// Read a root slot without evaluating array keys, retaining undefined-variable warnings.
    pub(crate) fn read_root(&mut self, register: Reg, missing: Missing) -> Value {
        let slot = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.0 as usize))
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone());
        match slot {
            Some(value) => value,
            None => {
                if missing == Missing::Warn {
                    let name = self
                        .current()
                        .and_then(|frame| {
                            frame
                                .function()
                                .locals
                                .get(register.0 as usize)
                                .and_then(|name| frame.unit().names.get(name.index()))
                        })
                        .cloned()
                        .unwrap_or_default();
                    self.warn(format!("Undefined variable ${name}"));
                }
                Value::Null
            }
        }
    }
}
