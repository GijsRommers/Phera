//! `ReflectionNamedType` preserves `?int`; union members report their own names.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassKind, Context, MethodEntry, NativeResult, Object, Registry, Value,
};

use crate::{UNCLONEABLE, allocate, load, values};

#[derive(Clone)]
struct ReflectedType {
    text: String,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Named,
    Union,
    Intersection,
}

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::AbstractClass,
        interfaces: &["Stringable"],
        methods: const {
            &[
                MethodEntry::public("allowsNull", &[], |_, o, _| {
                    Ok(Value::Bool(allows_null(&reflected(o)?.text)))
                })
                .tentative_return("bool"),
                MethodEntry::public("__toString", &[], |_, o, _| {
                    Ok(Value::from(reflected(o)?.text))
                })
                .returning("string"),
            ]
        },
        hooks: Some(&UNCLONEABLE),
        ..ClassEntry::class("ReflectionType")
    });
    registry.class(ClassEntry {
        parent: Some("ReflectionType"),
        methods: const {
            &[
                MethodEntry::public("getName", &[], |_, o, _| {
                    Ok(Value::from(reflected(o)?.text.trim_start_matches('?')))
                })
                .tentative_return("string"),
                MethodEntry::public("isBuiltin", &[], |_, o, _| {
                    let ty = reflected(o)?;
                    Ok(Value::Bool(is_builtin(ty.text.trim_start_matches('?'))))
                })
                .tentative_return("bool"),
            ]
        },
        ..ClassEntry::class("ReflectionNamedType")
    });
    for name in ["ReflectionUnionType", "ReflectionIntersectionType"] {
        registry.class(ClassEntry {
            parent: Some("ReflectionType"),
            methods: const { &[MethodEntry::public("getTypes", &[], members).returning("array")] },
            ..ClassEntry::class(name)
        });
    }
}

pub(crate) fn make(context: &mut dyn Context, declared: Option<&str>) -> NativeResult {
    let Some(declared) = declared else {
        return Ok(Value::Null);
    };
    let (kind, text) = if declared.contains('|') {
        let items = declared.split('|').collect::<Vec<_>>();
        if let [first, "null"] | ["null", first] = items.as_slice()
            && !first.contains('&')
        {
            (Kind::Named, format!("?{first}"))
        } else {
            (Kind::Union, normalize_union(items))
        }
    } else if declared.contains('&') {
        (
            Kind::Intersection,
            declared.trim_matches(['(', ')']).to_owned(),
        )
    } else {
        (Kind::Named, declared.to_owned())
    };
    let name = match kind {
        Kind::Named => "ReflectionNamedType",
        Kind::Union => "ReflectionUnionType",
        Kind::Intersection => "ReflectionIntersectionType",
    };
    allocate(context, name, None, ReflectedType { text, kind })
}

fn reflected(this: Option<&Rc<Object>>) -> Result<ReflectedType, phera_extension_api::NativeError> {
    load(this)
}

fn members(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let ty = reflected(this)?;
    let separator = if ty.kind == Kind::Union { '|' } else { '&' };
    let mut members = Vec::new();
    for member in ty.text.split(separator) {
        members.push(make(context, Some(member))?);
    }
    values(members)
}

const BUILTIN_ORDER: &[&str] = &[
    "static", "callable", "object", "array", "string", "int", "float", "bool", "false", "true",
    "null",
];

fn normalize_union(mut members: Vec<&str>) -> String {
    members.sort_by_key(|name| {
        BUILTIN_ORDER
            .iter()
            .position(|candidate| candidate == name)
            .map_or(0, |position| position + 1)
    });
    members
        .into_iter()
        .map(|member| {
            if member.contains('&') && !member.starts_with('(') {
                format!("({member})")
            } else {
                member.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn allows_null(text: &str) -> bool {
    text.starts_with('?') || text == "mixed" || text.split('|').any(|name| name == "null")
}

pub(crate) fn is_builtin(name: &str) -> bool {
    matches!(name, "mixed" | "void" | "never" | "iterable")
        || name != "static" && BUILTIN_ORDER.contains(&name)
}
