//! ReflectionObject($ghost)->getProperties() reads the instance when the filter includes public properties.

use std::rc::Rc;

use phera_extension_api::reflection::{ClassSelector, MemberInfo, MemberKind, Members};
use phera_extension_api::{ArrayKey, Context, NativeResult, Object, PhpArray, Value, args};

use super::{matches_filter, member_object};
use crate::class::reflected_class;
use crate::engine;

/// A filter that cannot select dynamic properties needs only the class metadata.
pub(super) fn properties(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let class = reflected_class(this)?;
    let kind = MemberKind::Property;
    let declared = engine(context)?.reflect_members(ClassSelector::Linked(class.info.id), kind);
    let mut result = PhpArray::with_capacity(declared.len());
    for member in declared
        .iter()
        .filter(|member| matches_filter(member, arguments))
    {
        result.push(member_object(context, member.clone(), kind)?)?;
    }
    if args::int(arguments, 0).is_none_or(|filter| filter & 1 != 0)
        && let Some(object) = class.object.and_then(|object| object.upgrade())
    {
        return with_instance(context, object, move |context, object| {
            let slots = object.to_array();
            for member in instance_members(&declared, &class.info.name, &slots) {
                result.push(member_object(context, member, kind)?)?;
            }
            Ok(result.into())
        });
    }
    Ok(result.into())
}

pub(crate) fn with_instance(
    context: &mut dyn Context,
    object: Rc<Object>,
    read: impl FnOnce(&mut dyn Context, Rc<Object>) -> NativeResult + 'static,
) -> NativeResult {
    if object.lazy().is_some() {
        engine(context)?.reflect_after_initialization(object, Box::new(read))
    } else {
        read(context, object)
    }
}

fn instance_members<'a>(
    declared: &'a Members,
    class: &'a str,
    slots: &'a PhpArray,
) -> impl Iterator<Item = Rc<MemberInfo>> + use<'a> {
    slots.iter().filter_map(move |(key, _)| {
        let ArrayKey::String(key) = key else {
            return None;
        };
        if key.as_bytes().starts_with(b"\0") {
            return None;
        }
        let name = key.to_string_lossy();
        (!declared.iter().any(|property| property.name == name))
            .then(|| dynamic_property(class, name))
    })
}

pub(super) fn append_instance_members(result: &mut Members, class: &str, object: &Object) {
    let slots = object.to_array();
    let properties: Vec<_> = instance_members(result, class, &slots).collect();
    if !properties.is_empty() {
        Rc::make_mut(result).extend(properties);
    }
}

pub(super) fn dynamic_property(class: &str, name: String) -> Rc<MemberInfo> {
    Rc::new(MemberInfo {
        name,
        class: class.into(),
        lookup_class: class.into(),
        modifiers: 1,
        dynamic: true,
        ..MemberInfo::default()
    })
}
