//! `spl_autoload('App\\Thing')` searches for `app/thing.inc` and `app/thing.php`.

use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// SPL silently skips missing files and executes each located file at most once.
    pub(crate) async fn default_autoload(
        &mut self,
        name: &str,
        extensions: Option<String>,
    ) -> Result<(), NativeError> {
        let base = name.strip_prefix('\\').unwrap_or(name).to_ascii_lowercase();
        let path = base.replace('\\', std::path::MAIN_SEPARATOR_STR);
        let extensions = extensions.unwrap_or_else(|| self.autoload_extensions().to_owned());
        if extensions.is_empty() {
            return Ok(());
        }
        let stop = self.call_stack.depth();
        for extension in extensions.split_terminator(',') {
            let candidate = format!("{path}{extension}");
            let Ok(unit) = self.load_include(candidate.as_bytes()).await? else {
                continue;
            };
            if self.include_state.contains(&unit.file) {
                continue;
            }
            self.include_state.record(unit.file.clone());
            self.enter_scoped_unit(unit, None)?;
            self.interpret(stop).await?;
            if self.classes.entries.contains_key(&base) {
                break;
            }
        }
        Ok(())
    }
}

impl Vm {
    pub(crate) fn include_path_intrinsic(
        &mut self,
        intrinsic: crate::intrinsics::Intrinsic,
        mut arguments: Vec<phera_value::Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> phera_extension_api::NativeResult {
        use phera_extension_api::{Param, ParamType};
        use phera_value::Value;
        let set = matches!(intrinsic, crate::intrinsics::Intrinsic::SetIncludePath);
        let (name, parameters): (_, &[Param]) = if set {
            (
                "set_include_path",
                &[Param::new("include_path", ParamType::String)],
            )
        } else {
            ("get_include_path", &[])
        };
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(name, parameters, &mut arguments, vm)
        })?;
        if let Some(Value::String(path)) = arguments.first() {
            return Ok(Value::from(self.set_include_path(path.clone())));
        }
        Ok(Value::from(self.include_path()))
    }
}

impl Vm {
    pub(crate) fn include_path(&self) -> phera_value::PhpString {
        self.include_path.clone().unwrap_or_else(|| ".".into())
    }

    pub(crate) fn set_include_path(
        &mut self,
        path: phera_value::PhpString,
    ) -> phera_value::PhpString {
        self.include_path
            .replace(path)
            .unwrap_or_else(|| ".".into())
    }
}
