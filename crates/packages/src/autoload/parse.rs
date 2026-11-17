//! Class-map-only parsing of PHP containing expressions Phera does not execute yet.
//!
//! A method containing ``return `whoami`;`` still contributes its enclosing class.
//! Only complete shell-expression literals and deprecated interpolation openers change;
//! declaration tokens remain intact and the Phera parser discovers every class.

use phera_ast::Program;
use phera_parser::ParseError;

pub(super) fn source(text: &str) -> Result<Program, ParseError> {
    let mut text = std::borrow::Cow::Borrowed(text);
    let mut replacements = 0;
    loop {
        match phera_parser::parse_on_current_stack(&text) {
            Ok(program) => return Ok(program),
            Err(error) if replacements < 128 => {
                let start = error.span.start;
                if error.message == "unsupported syntax: backtick shell execution" {
                    let mut escaped = false;
                    let end = text[start + 1..]
                        .char_indices()
                        .find_map(|(index, character)| {
                            if escaped {
                                escaped = false;
                                return None;
                            }
                            if character == '\\' {
                                escaped = true;
                                return None;
                            }
                            (character == '`').then_some(start + index + 2)
                        });
                    let Some(end) = end else {
                        return Err(error);
                    };
                    text.to_mut().replace_range(start..end, "''");
                } else if error.message.starts_with("unsupported syntax: `${...}`") {
                    text.to_mut().replace_range(start..start + 2, "{$");
                } else {
                    return Err(error);
                }
                replacements += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_declarations_around_shell_expressions_and_legacy_interpolation() {
        let source = "<?php namespace Acme; class Runner {\n\
            function run($id) { echo \"${id}\"; return `echo 'class Fake {}'`; }\n\
            } interface Contract {} trait Helper {} enum State {}";
        let program = super::source(source).unwrap();
        let mut names = Vec::new();
        super::super::scan::declarations(&program.statements, "", &mut names);
        assert_eq!(
            names,
            [
                "Acme\\Runner",
                "Acme\\Contract",
                "Acme\\Helper",
                "Acme\\State"
            ]
        );
    }
}
