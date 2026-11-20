//! `stack_get -i 2 -d 0` describes the innermost suspended PHP frame.

use super::protocol::{Command, file_uri};
use crate::Vm;
use quick_xml::escape::escape;
use std::fmt::Write;

impl Vm {
    pub(super) fn debug_stack(&self, command: &Command, pause_line: Option<u32>) -> String {
        let mut body = String::new();
        let requested = command.number::<usize>(b'd');
        let frames = self.frames();
        let depth = frames.len();
        for (index, frame) in frames.rev().enumerate() {
            if requested.is_some_and(|selected| selected != index) {
                continue;
            }
            let file = file_uri(&frame.unit().file);
            let line = if index == 0 {
                pause_line.map_or_else(|| frame.line(), |line| line as usize)
            } else {
                frame.line()
            };
            let name = if frame.function_id().is_none() && index + 1 < depth {
                depth
                    .checked_sub(index + 2)
                    .and_then(|caller| self.frame_at(caller))
                    .and_then(crate::exception_traces::scoped_unit_name)
                    .unwrap_or("{main}")
                    .to_owned()
            } else {
                let name = self.code.function_name(frame.frame.function);
                if frame.this().is_some() && !name.starts_with("{closure:") {
                    name.replacen("::", "->", 1)
                } else {
                    name.to_owned()
                }
            };
            let _ = write!(
                body,
                r#"<stack where="{}" level="{index}" type="file" filename="{}" lineno="{line}"/>"#,
                escape(&name),
                escape(&file)
            );
        }
        if body.is_empty() && requested.is_some() {
            return command.error(301, "invalid stack depth");
        }
        command.response("", &body)
    }
}
