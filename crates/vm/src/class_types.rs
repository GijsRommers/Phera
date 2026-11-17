//! Class declarations use the same ancestry test as instanceof, including intersections.

use phera_bytecode::Type;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Mirrors matches_class_type for absolute declarations, avoiding name hashing
    /// after the first check for a unit TypeId and receiver class. Relative types
    /// retain the scope-dependent resolver on every call.
    pub(crate) fn matches_cached_class_type(
        &self,
        value: &Value,
        declaration: phera_bytecode::TypeId,
        unit: &phera_bytecode::CompiledUnit,
        cache: &crate::unit_caches::UnitCache,
    ) -> bool {
        let Some(ty) = unit.types.get(declaration.index()) else {
            return false;
        };
        let Some(slot) = cache.class_types.get(declaration.index()).filter(|_| {
            cache.type_scopes.get(declaration.index())
                == Some(&crate::types::DeclarationScope::Absolute)
        }) else {
            return self.matches_class_type(value, ty);
        };
        let Value::Object(object) = value else {
            return self.matches_class_type(value, ty);
        };
        if let Some(entry) = &*slot.borrow()
            && std::rc::Rc::ptr_eq(&entry.class, &object.class)
        {
            return entry.matches;
        }
        let matches = self.matches_class_type(value, ty);
        *slot.borrow_mut() = Some(crate::unit_caches::ClassTypeEntry {
            class: object.class.clone(),
            matches,
        });
        matches
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
        // Mirrors the resolved-name membership path below without reparsing an
        // already seen absolute declaration on every parameter and return check.
        if !matches!(declaration.display.as_str(), "self" | "parent" | "static")
            && let Some(result) = class.cached_declaration_result(&declaration.display)
        {
            return result;
        }
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
            resolved.is_some_and(|name| class.matches_declaration(name))
        };
        let names = declaration.display.trim_start_matches('?').split('|');
        names
            .into_iter()
            .any(|alternative| alternative.split('&').all(matches))
    }
}

/// Whether the unit's cache already remembers that the class of `value`
/// satisfies the class type `declaration`, by identity of the class. Only a
/// remembered "yes" counts: anything else is left to the general check, which
/// also fills the cache.
pub(crate) fn known_class_match(
    cache: &crate::unit_caches::UnitCache,
    declaration: phera_bytecode::TypeId,
    value: &Value,
) -> bool {
    let Value::Object(object) = value else {
        return false;
    };
    let index = declaration.index();
    cache.type_scopes.get(index) == Some(&crate::types::DeclarationScope::Absolute)
        && cache.class_types.get(index).is_some_and(|slot| {
            slot.borrow().as_ref().is_some_and(|entry| {
                entry.matches && std::rc::Rc::ptr_eq(&entry.class, &object.class)
            })
        })
}
