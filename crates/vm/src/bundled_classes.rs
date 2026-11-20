//! Classes Phera ships written in PHP.
//!
//! ```php
//! // vendor/autoload_runtime.php, with APP_RUNTIME=Phera\Symfony\Runtime:
//! $runtime = new $_SERVER['APP_RUNTIME']([...]);   // no file of the
//! // application declares it: Phera compiles the source it ships, once
//! ```
//!
//! A class in Phera's own namespace that is not declared yet is looked for
//! among the bundled sources (`Registry::bundled_class`) before any
//! autoloader runs: the namespace is Phera's, and no autoloader of the
//! application can know it. Loading one is like an include that is not
//! listed by `get_included_files()`.

use std::rc::Rc;

use phera_extension_api::NativeError;

use crate::Vm;
use crate::classes::Class;

impl Vm {
    /// Declares the bundled class `name` if Phera ships one.
    pub(crate) async fn load_bundled_class(
        &mut self,
        name: &str,
    ) -> Result<Option<Rc<Class>>, NativeError> {
        let lowercase = name.to_ascii_lowercase();
        if !lowercase.starts_with("phera\\") {
            return Ok(None);
        }
        let Some(source) = self.registry.bundled_source(&lowercase) else {
            return Ok(None);
        };
        let Some(loader) = self.include_state.loader() else {
            return Ok(None);
        };
        // Its own declaration may look the name up again (a parent that is
        // missing): that lookup goes on to the autoloaders.
        let guard = format!("bundled {lowercase}");
        if !self.autoload.active.insert(guard.clone()) {
            return Ok(None);
        }
        let result = self.run_bundled(loader, source).await;
        self.autoload.active.remove(&guard);
        result?;
        Ok(self.classes.get(name))
    }

    async fn run_bundled(
        &mut self,
        loader: std::rc::Rc<dyn crate::CodeLoader>,
        source: phera_extension_api::BundledSource,
    ) -> Result<(), NativeError> {
        let unit = match loader.load_bundled(source).await {
            Ok(unit) => unit,
            Err(crate::CodeLoadError::Compile(error)) => {
                return Err(self.dynamic_compile_failure(error).await);
            }
            Err(crate::CodeLoadError::Unavailable(message)) => {
                return Err(NativeError::error(message));
            }
        };
        let stop = self.call_stack.depth();
        self.enter_scoped_unit(unit, None)?;
        self.interpret(stop).await?;
        Ok(())
    }
}
