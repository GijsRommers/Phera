//! Class declarations use the same ancestry test as instanceof, including intersections.

use phera_bytecode::Type;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Resolve declaration keywords in lexical scope and static against the called class.
    pub(crate) fn matches_class_type(&self, value: &Value, declaration: &Type) -> bool {
        let Value::Object(object) = &*value.deref() else {
            return false;
        };
        let Some(class) = self.classes.get(object.class.name()) else {
            return false;
        };
        let context = self.current().and_then(|frame| frame.method.as_ref());
        let matches = |name: &str| {
            let resolved = match name.trim_matches(['(', ')']) {
                "self" => context.map(|context| context.scope.name.as_str()),
                "static" => context.map(|context| context.called.name.as_str()),
                "parent" => context
                    .and_then(|context| context.scope.parent.as_ref())
                    .map(|parent| parent.name.as_str()),
                name => Some(name),
            };
            resolved.is_some_and(|name| class.is_a(name))
        };
        let names = declaration.display.trim_start_matches('?').split('|');
        names
            .into_iter()
            .any(|alternative| alternative.split('&').all(matches))
    }
}
