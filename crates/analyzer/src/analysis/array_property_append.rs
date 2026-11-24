//! `$this->entries[] = $entry` adds an integer key to `array<K, V>`.
//! The class's caller may have chosen string keys for K.

use super::*;
use crate::{scope::Place, types::CollectionKind};

impl Analyzer<'_> {
    pub(super) fn check_template_key_append(
        &mut self,
        property: &Expression,
        value: &ValueType,
        scope: &Scope,
    ) {
        if self.level < 7 {
            return;
        }
        let ExpressionKind::Property {
            target,
            name: MemberName::Identifier(name),
            nullsafe: false,
        } = &property.kind
        else {
            return;
        };
        if !matches!(&target.kind, ExpressionKind::Variable(Variable::Named(name)) if name == "this")
        {
            return;
        }
        let Some((owner, property_info)) =
            Place::of(property).and_then(|place| self.resolved_property(&place, scope))
        else {
            return;
        };
        if self.context.class.as_ref() != Some(&owner.name) {
            return;
        }
        let ValueType::Collection(collection) = property_info.effective_type(self.index) else {
            return;
        };
        let ValueType::Template(key) = &collection.key else {
            return;
        };
        if collection.kind != CollectionKind::Array
            || *value != collection.value
            || !owner.documented.template_names.contains(&key.name)
        {
            return;
        }
        let arguments = owner
            .documented
            .template_names
            .iter()
            .map(|name| {
                template_name(
                    name,
                    owner
                        .documented
                        .templates
                        .bound(name)
                        .unwrap_or(&ValueType::Mixed),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let key = template_name(&key.name, &key.bound);
        let entry = match &collection.value {
            ValueType::Template(value) => template_name(&value.name, &value.bound),
            value => value.to_string(),
        };
        let class = &owner.name;
        let field = &name.name;
        self.diagnose(
            "assign.propertyType",
            property.span,
            format!("Property {class}<{arguments}>::${field} (array<{key}, {entry}>) does not accept array<int|{key}, {entry}>."),
        );
    }
}

fn template_name(name: &str, bound: &ValueType) -> String {
    match bound {
        ty if ty.is_mixed() => name.into(),
        ValueType::Union(arms) | ValueType::BenevolentUnion(arms) => {
            let bound = super::member_checks::receiver_name(&ValueType::Union(arms.clone()));
            format!("{name} of ({bound})")
        }
        _ => format!("{name} of {bound}"),
    }
}
