//! PHP supplies enum members: `E::cases()`, `E::from("a")`, `$case->name`.
//! These facts are implicit in the language rather than written in the class AST.

use crate::{index::*, names::Names, types::ValueType};
use phera_ast::{ClassDeclaration, ClassKind, Modifiers};

pub fn add_members(info: &mut ClassInfo, class: &ClassDeclaration, names: &Names) {
    if class.kind != ClassKind::Enum {
        return;
    }
    info.interfaces.push("UnitEnum".into());
    info.properties.insert(
        "name".into(),
        PropertyInfo {
            ty: ValueType::String,
            documented_type: None,
            null_default: false,
            modifiers: Modifiers::default(),
            location: info.location.clone(),
        },
    );
    let mut cases = Signature::new(&[], None, names, info.location.clone());
    cases.returns = ValueType::Array;
    cases.modifiers.is_static = true;
    info.methods.insert("cases".into(), cases);
    if let Some(backing) = &class.backing_type {
        info.interfaces.push("BackedEnum".into());
        let ty = ValueType::declared(Some(backing), names);
        info.properties.insert(
            "value".into(),
            PropertyInfo {
                ty: ty.clone(),
                documented_type: None,
                null_default: false,
                modifiers: Modifiers::default(),
                location: info.location.clone(),
            },
        );
        for method in ["from", "tryfrom"] {
            let mut signature = Signature::new(&[], None, names, info.location.clone());
            signature.modifiers.is_static = true;
            signature.parameters.push(ParameterInfo {
                name: "value".into(),
                ty: ty.clone(),
                optional: false,
                default_is_null: false,
                default_boolean: None,
                variadic: false,
                by_reference: false,
                closure_receiver: None,
            });
            signature.returns = ValueType::Class(info.name.clone());
            if method == "tryfrom" {
                signature.returns = signature.returns.join(&ValueType::Null);
            }
            info.methods.insert(method.into(), signature);
        }
    }
}
