//! At level 7, `($readerOrWriter)->read()` must work on each union member.
//! At level 8, nullable receivers require a guard or the nullsafe operator.

use super::receiver_types::Receivers;
use super::*;

impl Analyzer<'_> {
    pub fn strict_receiver(&self, ty: &ValueType) -> bool {
        if self.strict_mixed(ty) {
            return true;
        }
        let ValueType::Union(arms) = ty else {
            return false;
        };
        arms.iter().any(|arm| {
            if *arm == ValueType::Null {
                return self.level >= 8;
            }
            let receivers = Receivers::of(arm);
            self.level >= 7
                && receivers.known_scalar
                && receivers.classes.is_empty()
                && !receivers.uncertain
        })
    }

    pub fn missing_method_arm(&mut self, ty: &ValueType, name: &str, access: Access) -> bool {
        let ValueType::Union(arms) = ty else {
            return false;
        };
        self.level >= 7
            && arms.iter().any(|arm| {
                let receivers = Receivers::of(arm);
                !receivers.uncertain
                    && !receivers.classes.is_empty()
                    && receivers.classes.iter().all(|class| {
                        self.dependencies.resolve(self.index, class);
                        self.index.method(class, name).is_none()
                            && self.index.doc_method(class, name).is_none()
                            && self.can_report_missing_method(class, name, access)
                    })
            })
    }

    pub fn missing_property_arm(&mut self, ty: &ValueType, name: &str) -> bool {
        let ValueType::Union(arms) = ty else {
            return false;
        };
        self.level >= 7
            && arms.iter().any(|arm| {
                let receivers = Receivers::of(arm);
                !receivers.uncertain
                    && !receivers.classes.is_empty()
                    && receivers.classes.iter().all(|class| {
                        self.dependencies.resolve(self.index, class);
                        self.index.property(class, name).is_none()
                            && self.index.doc_property_type(class, name).is_none()
                            && self.can_report_missing_property(
                                class,
                                name,
                                PropertyAccess::Instance,
                            )
                    })
            })
    }
}

pub(super) fn receiver_name(ty: &ValueType) -> String {
    if let ValueType::Union(arms) = ty {
        let mut names: Vec<_> = arms
            .iter()
            .map(|arm| (type_order(arm), type_name(arm)))
            .collect();
        names.sort_unstable();
        names
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
            .join("|")
    } else {
        type_name(ty)
    }
}

fn type_name(ty: &ValueType) -> String {
    if let ValueType::Collection(collection) = ty
        && collection.kind == crate::types::CollectionKind::Array
        && let ValueType::Union(keys) | ValueType::BenevolentUnion(keys) = &collection.key
        && keys.len() == 2
        && keys.contains(&ValueType::Int)
        && keys.contains(&ValueType::String)
    {
        let prefix = if collection.non_empty {
            "non-empty-"
        } else {
            ""
        };
        return format!("{prefix}array<{}>", collection.value);
    }
    ty.to_string()
}

fn type_order(ty: &ValueType) -> u8 {
    match ty {
        ValueType::Class(_) | ValueType::Generic(_) | ValueType::Intersection(_) => 1,
        ValueType::Null => 2,
        _ => 0,
    }
}
