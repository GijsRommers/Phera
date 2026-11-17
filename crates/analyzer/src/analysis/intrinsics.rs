//! PHP scope intrinsics: `compact("x")` reads a variable by its string name.
//! Unknown facts stay silent; no PHP code is loaded or executed.

use super::*;

impl Analyzer<'_> {
    pub fn intrinsic(&mut self, name: &str, arguments: &Arguments, scope: &mut Scope) {
        let Arguments::List(arguments) = arguments else {
            return;
        };
        match name.to_lowercase().as_str() {
            "extract" | "parse_str" if arguments.len() == 1 => scope.unknown_writes(),
            "extract" => scope.unknown_writes(),
            "compact" => {
                for argument in arguments {
                    let mut values = vec![&argument.value];
                    while let Some(value) = values.pop() {
                        match &value.kind {
                            ExpressionKind::Literal(Literal::String(bytes)) => {
                                if let Ok(name) = std::str::from_utf8(bytes) {
                                    self.read(name, scope, argument.span, false);
                                }
                            }
                            ExpressionKind::Array(items) => {
                                values.extend(items.iter().map(|i| &i.value))
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
