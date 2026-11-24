//! `feature_set -i 1 -n max_children -v 32` limits how many children an IDE page contains.

use quick_xml::escape::escape;

use super::protocol::Command;

#[derive(Clone)]
pub(super) struct Features {
    pub(super) max_children: usize,
    pub(super) max_data: usize,
    pub(super) max_depth: usize,
    pub(super) extended_properties: bool,
    pub(super) notify_ok: bool,
    pub(super) resolved_breakpoints: bool,
    pub(super) breakpoint_details: bool,
    pub(super) show_hidden: bool,
    pub(super) multiple_sessions: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            max_children: 32,
            max_data: 1024,
            max_depth: 1,
            extended_properties: false,
            notify_ok: false,
            resolved_breakpoints: false,
            breakpoint_details: false,
            show_hidden: false,
            multiple_sessions: false,
        }
    }
}

impl Features {
    pub(super) fn get(&self, command: &Command) -> String {
        let name = command.get(b'n').unwrap_or_default();
        let value = match name {
            "max_children" => self.max_children.to_string(),
            "max_data" => self.max_data.to_string(),
            "max_depth" => self.max_depth.to_string(),
            "extended_properties" => digit(self.extended_properties).into(),
            "notify_ok" => digit(self.notify_ok).into(),
            "resolved_breakpoints" => digit(self.resolved_breakpoints).into(),
            "breakpoint_details" => digit(self.breakpoint_details).into(),
            "show_hidden" => digit(self.show_hidden).into(),
            "multiple_sessions" => digit(self.multiple_sessions).into(),
            "language_name" | "breakpoint_languages" => "PHP".into(),
            "language_version" => phera_extension_api::PHP_VERSION.into(),
            "encoding" | "supported_encodings" => "UTF-8".into(),
            "data_encoding" => "base64".into(),
            "protocol_version" => "1".into(),
            "supports_async" => "1".into(),
            "language_supports_threads" => "0".into(),
            "breakpoint_types" => "line conditional call return exception".into(),
            "status" | "run" | "step_into" | "step_over" | "step_out" | "stop" | "detach"
            | "break" | "breakpoint_set" | "breakpoint_get" | "breakpoint_update"
            | "breakpoint_remove" | "breakpoint_list" | "stack_depth" | "stack_get"
            | "context_names" | "context_get" | "typemap_get" | "property_get"
            | "property_value" | "property_set" | "source" | "eval" => String::new(),
            _ => {
                return command.response(
                    &format!(r#" feature_name="{}" supported="0""#, escape(name)),
                    "",
                );
            }
        };
        command.response(
            &format!(r#" feature_name="{}" supported="1""#, escape(name)),
            &value,
        )
    }

    pub(super) fn set(&mut self, command: &Command) -> String {
        let name = command.get(b'n').unwrap_or_default();
        let value = command.get(b'v').unwrap_or_default();
        let success = match name {
            "max_children" => limit(&mut self.max_children, value, 65536),
            "max_data" => limit(&mut self.max_data, value, usize::MAX),
            "max_depth" => limit(&mut self.max_depth, value, 128),
            "extended_properties" => flag(&mut self.extended_properties, value),
            "notify_ok" => flag(&mut self.notify_ok, value),
            "resolved_breakpoints" => flag(&mut self.resolved_breakpoints, value),
            "breakpoint_details" => flag(&mut self.breakpoint_details, value),
            "show_hidden" => flag(&mut self.show_hidden, value),
            "multiple_sessions" => flag(&mut self.multiple_sessions, value),
            "encoding" => value.eq_ignore_ascii_case("UTF-8"),
            _ => false,
        };
        command.response(
            &format!(
                r#" feature="{}" success="{}""#,
                escape(name),
                digit(success)
            ),
            "",
        )
    }
}

fn limit(setting: &mut usize, value: &str, max: usize) -> bool {
    match value.parse::<usize>() {
        Ok(value) if value <= max => {
            *setting = value;
            true
        }
        _ => false,
    }
}

fn flag(setting: &mut bool, value: &str) -> bool {
    match value {
        "0" | "1" => {
            *setting = value == "1";
            true
        }
        _ => false,
    }
}

pub(super) fn digit(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}
