//! Magic declarations are checked before the script executes.
//!
//! `function __get(string $name): mixed` accepts a property name, while
//! `function __get(int $name)` is a compile error.

use phera_ast::{Method, Visibility};
use phera_bytecode::DeclarationWarning;

use crate::{CompileError, Compiler};

/// Magic methods have fixed signature rules, apart from __invoke's arguments.
#[derive(Clone, Copy)]
enum Signature<'a> {
    Invoke,
    Fixed {
        parameters: &'a [Option<&'a str>],
        returns: &'a str,
        kind: Kind,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Instance,
    Static,
    CloneMethod,
}

use Kind::{CloneMethod, Instance, Static};

fn signature(name: &str) -> Option<Signature<'static>> {
    let (parameters, returns, kind): (&[Option<&str>], _, _) = match name {
        "__get" => (&[Some("string")], "", Instance),
        "__set" => (&[Some("string"), None], "void", Instance),
        "__isset" => (&[Some("string")], "bool", Instance),
        "__unset" => (&[Some("string")], "void", Instance),
        "__call" => (&[Some("string"), Some("array")], "", Instance),
        "__callstatic" => (&[Some("string"), Some("array")], "", Static),
        "__tostring" => (&[], "string", Instance),
        "__clone" => (&[], "void", CloneMethod),
        "__debuginfo" => (&[], "?array", Instance),
        "__set_state" => (&[Some("array")], "object", Static),
        "__serialize" | "__sleep" => (&[], "array", Instance),
        "__unserialize" => (&[Some("array")], "void", Instance),
        "__wakeup" => (&[], "void", Instance),
        "__invoke" => return Some(Signature::Invoke),
        _ => return None,
    };
    Some(Signature::Fixed {
        parameters,
        returns,
        kind,
    })
}

impl Compiler<'_> {
    pub(super) fn validate_magic_method(
        &mut self,
        class: &str,
        method: &Method,
    ) -> Result<(), CompileError> {
        let name = &method.name.name;
        if !name.starts_with("__") {
            return Ok(());
        }
        let Some(signature) = signature(&name.to_ascii_lowercase()) else {
            return Ok(());
        };
        let line = self.lines.line(method.name.span.start);
        let qualified = format!("{class}::{name}()");
        let file = self.file;
        let error = |message| CompileError {
            message,
            file: file.into(),
            line,
            parse: false,
            scanner_diagnostics: Vec::new(),
        };
        let kind = match signature {
            Signature::Invoke => Instance,
            Signature::Fixed {
                parameters, kind, ..
            } => {
                // PHP excludes a variadic tail when checking the fixed argument count.
                let actual = method.parameters.iter().filter(|param| !param.variadic);
                if actual.clone().count() != parameters.len() {
                    let count = match parameters.len() {
                        0 => "cannot take arguments",
                        1 => "must take exactly 1 argument",
                        _ => "must take exactly 2 arguments",
                    };
                    return Err(error(format!("Method {qualified} {count}")));
                }
                if actual.clone().any(|param| param.by_reference) {
                    return Err(error(format!(
                        "Method {qualified} cannot take arguments by reference"
                    )));
                }
                kind
            }
        };
        if method.modifiers.is_static != (kind == Static) {
            let message = if kind == Static {
                "must be static"
            } else {
                "cannot be static"
            };
            return Err(error(format!("Method {qualified} {message}")));
        }
        if kind != CloneMethod
            && matches!(
                method.modifiers.visibility,
                Some(Visibility::Private | Visibility::Protected)
            )
        {
            self.declaration_warnings.push(DeclarationWarning {
                message: format!("The magic method {qualified} must have public visibility"),
                line: line as u32,
            });
        }
        if let Signature::Fixed {
            parameters,
            returns,
            ..
        } = signature
        {
            for (index, (expected, parameter)) in
                parameters.iter().zip(&method.parameters).enumerate()
            {
                if let (Some(expected), Some(ty)) = (expected, &parameter.ty) {
                    let declared = self.declaration_type(ty);
                    if declared != "mixed"
                        && !declared
                            .trim_start_matches('?')
                            .split('|')
                            .any(|ty| ty == *expected)
                    {
                        return Err(error(format!(
                            "{qualified}: Parameter #{} (${}) must be of type {expected} when declared",
                            index + 1,
                            parameter.name
                        )));
                    }
                }
            }
            if !returns.is_empty()
                && let Some(ty) = &method.return_type
                && !valid_return(&self.declaration_type(ty), returns)
            {
                return Err(error(format!(
                    "{qualified}: Return type must be {returns} when declared"
                )));
            }
        }
        Ok(())
    }
}

fn valid_return(declared: &str, expected: &str) -> bool {
    if declared == "never" || declared == expected {
        return true;
    }
    match expected {
        "?array" => declared
            .split('|')
            .all(|member| matches!(member, "array" | "null")),
        "bool" => matches!(declared, "true" | "false"),
        "object" => {
            !declared.starts_with('?')
                && declared.split('|').all(|member| {
                    !matches!(
                        member,
                        "array"
                            | "bool"
                            | "callable"
                            | "false"
                            | "float"
                            | "int"
                            | "iterable"
                            | "mixed"
                            | "null"
                            | "string"
                            | "true"
                            | "void"
                    )
                })
        }
        _ => false,
    }
}
