//! Regexp validation reuses the registered PCRE match handler.
//!
//! ```php
//! filter_var('12', FILTER_VALIDATE_REGEXP, ['options' => ['regexp' => '/^\d+$/']]);
//! // '12'
//! ```

use std::sync::OnceLock;

use phera_extension_api::{
    Context, Diagnostics, LocalFuture, NativeError, NativeFn, NativeResult, PhpString, Registry,
    RequestState, SyncFn, Value,
};

use crate::options::Options;

static MATCH: OnceLock<SyncFn> = OnceLock::new();

pub fn valid(
    context: &mut dyn Context,
    input: &PhpString,
    options: &Options,
) -> Result<bool, NativeError> {
    let pattern = options
        .string("regexp")
        .ok_or_else(|| options.invalid("\"regexp\" option is missing"))?;
    let matcher = MATCH.get_or_init(|| {
        let mut registry = Registry::new();
        registry.load(&phera_ext_pcre::Pcre);
        match registry
            .get_function("preg_match")
            .map(|entry| entry.handler)
        {
            Some(NativeFn::Sync(handler)) => handler,
            _ => unreachable!("the PCRE extension registers a sync preg_match"),
        }
    });
    let mut context = MatchContext {
        caller: context,
        function: options.call.name(),
        match_state: RequestState::default(),
        compile_failed: false,
    };
    // A third argument makes preg_match stop after its first match, even if empty.
    let result = matcher(
        &mut context,
        &[
            Value::String(pattern),
            Value::String(input.clone()),
            Value::Null,
        ],
    )?;
    Ok(matches!(result, Value::Int(1)))
}

/// PHP changes preg_last_error on a compilation failure. The outcome of a
/// compiled pattern's internal filter match stays private to that match.
struct MatchContext<'a> {
    caller: &'a mut dyn Context,
    function: &'static str,
    match_state: RequestState,
    compile_failed: bool,
}

impl Context for MatchContext<'_> {
    fn write(&mut self, bytes: &[u8]) {
        self.caller.write(bytes);
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self
    }

    fn state(&mut self) -> &mut RequestState {
        if self.compile_failed {
            self.caller.state()
        } else {
            &mut self.match_state
        }
    }

    fn strict_types(&self) -> bool {
        self.caller.strict_types()
    }

    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        self.caller.call(callable, arguments)
    }
}

impl Diagnostics for MatchContext<'_> {
    fn warning(&mut self, message: String) {
        if let Some(message) = message.strip_prefix("preg_match(): ") {
            self.compile_failed = true;
            self.caller
                .warning(format!("{}(): {message}", self.function));
        } else {
            self.caller.warning(message);
        }
    }

    fn notice(&mut self, message: String) {
        self.caller.notice(message);
    }

    fn deprecated(&mut self, message: String) {
        self.caller.deprecated(message);
    }
}
