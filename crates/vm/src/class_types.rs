//! Class declarations use the same ancestry test as instanceof, including intersections.

use phera_bytecode::Type;
use phera_value::Value;

use crate::Vm;
use crate::classes::ClassTable;
use crate::code_table::{CodeTable, FunctionIndex};

/// Direct zero-cache check for absolute class, union (`A|B`), and intersection
/// (`A&B`) declarations by pre-interned `NameKey` and pointer-walk ancestry
/// (`Class::derives_from`). DNF declarations (`(A&B)|C`), relative keywords
/// (`self`/`parent`/`static`), class aliases, and deep hierarchies return `false`
/// so callers can fall back to [`Vm::matches_class_type`].
#[inline]
pub(crate) fn fast_class_type_matches(
    code: &CodeTable,
    classes: &ClassTable,
    function: FunctionIndex,
    declaration: phera_bytecode::TypeId,
    value: &Value,
) -> bool {
    let Value::Object(object) = value else {
        return false;
    };
    let Some(class) = object
        .class
        .as_any()
        .downcast_ref::<crate::classes::Class>()
    else {
        return false;
    };
    let cache = code.cache(function);
    if cache.type_scopes.get(declaration.index()) != Some(&crate::types::DeclarationScope::Absolute)
    {
        return false;
    }
    let unit = code.unit(function);
    let Some(ty) = unit.types.get(declaration.index()) else {
        return false;
    };
    if ty.classes.is_empty() || ty.display.contains('(') {
        return false;
    }
    let check = |name: &phera_bytecode::NameId| {
        classes
            .by_key(code.unit_name_key(function, *name))
            .is_some_and(|target| class.derives_from(target) == Some(true))
    };
    if ty.intersection {
        ty.classes.iter().all(check)
    } else {
        ty.classes.iter().any(check)
    }
}

impl Vm {
    /// Checks an object against a compiled declaration directly via `NameKey`
    /// and `Class::derives_from`, falling back to [`Self::matches_class_type`]
    /// for relative keywords, DNF types, class aliases, and deep hierarchies.
    #[inline]
    pub(crate) fn matches_declared_class_type(
        &self,
        function: FunctionIndex,
        declaration: phera_bytecode::TypeId,
        value: &Value,
    ) -> bool {
        if fast_class_type_matches(&self.code, &self.classes, function, declaration, value) {
            return true;
        }
        self.code
            .unit(function)
            .types
            .get(declaration.index())
            .is_some_and(|ty| self.matches_class_type(value, ty))
    }

    /// Resolve declaration keywords in lexical scope and static against the called class.
    pub(crate) fn matches_class_type(&self, value: &Value, declaration: &Type) -> bool {
        let Value::Object(object) = &*value.deref() else {
            return false;
        };
        let Some(class) = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
        else {
            return false;
        };
        let context = self.current().and_then(|frame| frame.method());
        let context = context.as_ref();
        let matches = |name: &str| {
            let resolved = match name.trim_matches(['(', ')']) {
                "self" => context.map(|context| context.scope.name.as_str()),
                "static" => context.map(|context| context.called.name.as_str()),
                "parent" => context
                    .and_then(|context| context.scope.parent.as_ref())
                    .map(|parent| parent.name.as_str()),
                name => Some(name),
            };
            resolved.is_some_and(|name| {
                class.matches_declaration(name) || self.alias_matches(class, name)
            })
        };
        let names = declaration.display.trim_start_matches('?').split('|');
        names
            .into_iter()
            .any(|alternative| alternative.split('&').all(matches))
    }
}
