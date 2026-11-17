//! Closure parameter types follow rebound class scope: `Closure::bind($f, null, Foo::class)`.

use phera_bytecode::Type;

/// Resolve keywords before binding while the caller is still the active frame.
pub(crate) fn parameter_type(
    declaration: &Type,
    scope: crate::classes::class_table::ClassIndex,
    called: crate::classes::class_table::ClassIndex,
    classes: &crate::classes::ClassTable,
) -> Type {
    let mut declaration = declaration.clone();
    declaration.display = declaration
        .display
        .split_inclusive(['|', '&', '?'])
        .map(|part| {
            let name = part.trim_end_matches(['|', '&', '?']);
            let suffix = &part[name.len()..];
            let resolved = match name {
                "self" => classes.at(scope).name.as_str(),
                "parent" => classes
                    .at(scope)
                    .parent
                    .as_ref()
                    .map_or(name, |parent| parent.name.as_str()),
                "static" => classes.at(called).name.as_str(),
                _ => name,
            };
            format!("{resolved}{suffix}")
        })
        .collect();
    declaration
}
