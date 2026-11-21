//! Closure parameter types follow rebound class scope: `Closure::bind($f, null, Foo::class)`.

use phera_bytecode::Type;

use crate::classes::MethodContext;

/// Resolve keywords before binding while the caller is still the active frame.
pub(crate) fn parameter_type(declaration: &Type, context: &MethodContext) -> Type {
    let mut declaration = declaration.clone();
    let separators = ['|', '&', '?', '(', ')'];
    declaration.display = declaration
        .display
        .split_inclusive(separators)
        .map(|part| {
            let name = part.trim_end_matches(separators);
            let suffix = &part[name.len()..];
            let resolved = match name {
                "self" => context.scope.name.as_str(),
                "parent" => context
                    .scope
                    .parent
                    .as_ref()
                    .map_or(name, |parent| parent.name.as_str()),
                "static" => context.called.name.as_str(),
                _ => name,
            };
            format!("{resolved}{suffix}")
        })
        .collect();
    declaration
}
