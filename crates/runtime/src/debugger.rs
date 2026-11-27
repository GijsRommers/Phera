//! `?XDEBUG_TRIGGER=1` can start debugging on a server configured for triggered sessions.

use std::sync::Arc;

use phera_vm::{DebugConfig, DebugStart, DebugTimeLimit, Vm};

use crate::Runtime;
use crate::cache::{CodeCache, FileMode};

impl Runtime {
    pub fn with_debug(mut self, debug: DebugConfig) -> Self {
        if self.debug.is_none() {
            Vm::register_debugger(Arc::make_mut(&mut self.registry));
            Vm::register_debugger(Arc::make_mut(&mut self.web_registry));
            self.cache = CodeCache::new(self.registry.clone(), FileMode::Cli);
            self.web_cache = CodeCache::new(self.web_registry.clone(), FileMode::Web);
            if let Some(image) = &self.image {
                self.cache = self.cache.with_image(image.clone());
                self.web_cache = self.web_cache.with_image(image.clone());
            }
        }
        self.debug = Some(Arc::new(debug));
        self
    }

    #[cold]
    pub(crate) async fn connect_debugger(
        &self,
        vm: &mut Vm,
        file: &str,
        time_limit: Option<DebugTimeLimit>,
    ) {
        let Some(debug) = &self.debug else { return };
        debug.apply_ini(vm);
        let trigger = |name: &str| {
            vm.debugger_trigger(name).or_else(|| {
                self.environment
                    .get(name.as_bytes())
                    .map(|value| String::from_utf8_lossy(value).into_owned())
            })
        };
        let session = trigger("XDEBUG_SESSION");
        if debug.start == DebugStart::Always
            || trigger("XDEBUG_TRIGGER").is_some()
            || session.is_some()
            || self
                .environment
                .get(b"XDEBUG_CONFIG")
                .is_some_and(|value| !value.is_empty())
        {
            let key = if debug.idekey.is_empty() {
                session.as_deref().unwrap_or_default()
            } else {
                &debug.idekey
            };
            vm.connect_debugger(debug, file, key, time_limit).await;
        }
    }
}
