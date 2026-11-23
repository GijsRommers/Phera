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
            if self.classes.get_lowercase(&base).is_some() {
                break;
            }
        }
        Ok(())
    }
}

impl Vm {
    pub(crate) fn include_path(&self) -> phera_value::PhpString {
        self.ini_get(b"include_path").unwrap_or_else(|| ".".into())
    }
}
