//! `session_id('example')` sets the ID for this request alone.

use std::{collections::HashMap, rc::Rc};

use phera_extension_api::{Context, Object, PhpString, Value};

use crate::files::Files;

#[derive(Default)]
pub struct SessionState {
    pub settings: HashMap<&'static str, PhpString>,
    pub numbers: HashMap<&'static str, i64>,
    pub id: Option<PhpString>,
    pub active: bool,
    pub variables: Option<Value>,
    pub before: Option<Vec<u8>>,
    pub files: Option<Files>,
    pub user: Option<Rc<Object>>,
    pub user_open: bool,
    pub in_handler: bool,
    pub started: Option<(String, usize)>,
}

pub fn state(context: &mut dyn Context) -> &mut SessionState {
    context.state().get_or_default::<SessionState>()
}

pub fn active(context: &mut dyn Context) -> bool {
    context
        .state()
        .get::<SessionState>()
        .is_some_and(|session| session.active)
}

pub fn after_start(context: &mut dyn Context, message: &str) -> String {
    let Some((file, line)) = &state(context).started else {
        return message.into();
    };
    format!("{message} (started from {file} on line {line})")
}

pub fn after_output(context: &dyn Context, message: &str) -> String {
    match context.output_origin() {
        Some((file, line)) => format!("{message} (sent from {file} on line {line})"),
        None => message.into(),
    }
}

pub fn can_change(context: &mut dyn Context, function: &str, setting: &str, headers: bool) -> bool {
    let description = format!("{function}(): Session {setting} cannot be changed");
    let message = if active(context) {
        after_start(context, &format!("{description} when a session is active"))
    } else if headers && context.output_origin().is_some() {
        after_output(
            context,
            &format!("{description} after headers have already been sent"),
        )
    } else {
        return true;
    };
    context.warning(message);
    false
}

/// PHP's docref diagnostics distinguish a named call from module shutdown.
pub fn warning(context: &mut dyn Context, function: &str, message: &str) {
    let origin = if function == "PHP Request Shutdown" {
        function.to_owned()
    } else {
        format!("{function}()")
    };
    let message = if matches!(phera_ext_http::sapi(context), phera_ext_http::Sapi::Web) {
        message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#039;")
    } else {
        message.to_owned()
    };
    context.warning(format!("{origin}: {message}"));
}
