//! Read generated builtin declarations, such as `DateTime::format(string $format)`.
//! This data decoder never parses or executes user PHP.

use super::fields::*;
use crate::index::*;
use phera_ast::ClassKind;
use std::collections::BTreeMap;

pub fn decode(data: &str) -> Index {
    let mut index = Index::default();
    for line in data.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<_> = line.split('\t').collect();
        if f.len() < 2 {
            continue;
        }
        let name = f[1];
        match f[0] {
            "C" if f.len() == 10 => {
                index.classes.insert(name.into(), class(&f));
            }
            "F" if f.len() == 8 => {
                index.functions.insert(name.into(), signature(&f));
            }
            "M" if f.len() == 8 => {
                if let Some((class, method)) = name.split_once("::")
                    && let Some(class) = index.classes.get_mut(class)
                {
                    class.methods.insert(method.into(), signature(&f));
                }
            }
            "P" if f.len() == 5 => {
                if let Some((class, property)) = name.split_once("::")
                    && let Some(class) = index.classes.get_mut(class)
                {
                    class.properties.insert(
                        property.into(),
                        PropertyInfo {
                            ty: decode_type(f[2]),
                            documented_type: None,
                            null_default: false,
                            modifiers: modifiers(f[3]),
                            location: location(f[4]),
                        },
                    );
                }
            }
            "K" if f.len() == 5 => {
                if let Some((class, name)) = name.split_once("::")
                    && let Some(class) = index.classes.get_mut(class)
                {
                    class.constants.insert(name.into(), constant(&f));
                }
            }
            "G" if f.len() == 5 => {
                index.constants.insert(name.into(), constant(&f));
            }
            _ => {}
        }
    }
    index
}

fn class(f: &[&str]) -> ClassInfo {
    ClassInfo {
        name: f[2].into(),
        kind: match f[3] {
            "1" => ClassKind::Interface,
            "2" => ClassKind::Trait,
            "3" => ClassKind::Enum,
            _ => ClassKind::Class,
        },
        modifiers: modifiers(f[4]),
        parents: list(f[5]),
        interfaces: list(f[6]),
        traits: list(f[7]),
        adaptations: Vec::new(),
        methods: BTreeMap::new(),
        properties: BTreeMap::new(),
        constants: BTreeMap::new(),
        enum_cases: Default::default(),
        location: location(f[9]),
        open_properties: f[8] == "true",
        mapped_repository: None,
        documented: Default::default(),
    }
}

fn signature(f: &[&str]) -> Signature {
    let parameters = f[2]
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let p: Vec<_> = p.split(':').collect();
            (p.len() == 5).then(|| ParameterInfo {
                name: p[0].into(),
                ty: decode_type(p[1]),
                optional: p[2] == "true",
                default_is_null: false,
                variadic: p[3] == "true",
                by_reference: p[4] == "true",
                closure_receiver: None,
            })
        })
        .collect();
    Signature {
        pure: false,
        closure_object: true,
        returns_reference: false,
        parameters,
        returns: decode_type(f[3]),
        documentation: None,
        modifiers: modifiers(f[4]),
        abstract_body: f[5] == "true",
        accepts_extra: f[6] == "true",
        unbound_this: Vec::new(),
        global_effects: crate::global_effects::GlobalEffects::None,
        captured_writes: Vec::new(),
        location: location(f[7]),
    }
}

fn constant(f: &[&str]) -> ConstantInfo {
    ConstantInfo {
        ty: decode_type(f[2]),
        modifiers: modifiers(f[3]),
        location: location(f[4]),
    }
}
