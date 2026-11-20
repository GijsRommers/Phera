//! `$em->getRepository(Product::class)` uses the mapped repository for Product.
//! An ORM manager's `find(Product::class, $id)` returns Product or null.

use crate::{
    index::Index,
    names::Names,
    types::{Generic, ValueType},
};
use phera_ast::{Attribute, ExpressionKind, MemberName, UseKind};
use std::sync::Arc;

const MANAGER: &str = "Doctrine\\ORM\\EntityManagerInterface";
const REPOSITORY: &str = "Doctrine\\ORM\\EntityRepository";
const LAZY_REPOSITORY: &str =
    "Doctrine\\Bundle\\DoctrineBundle\\Repository\\LazyServiceEntityRepository";

pub fn stub_is_public(class: &str) -> bool {
    LAZY_REPOSITORY.eq_ignore_ascii_case(class)
}

const QUERY_RESULTS: &[(&str, &[&str])] = &[
    (
        "Doctrine\\ORM\\AbstractQuery",
        &[
            "getSingleScalarResult",
            "getSingleResult",
            "getOneOrNullResult",
        ],
    ),
    ("Doctrine\\DBAL\\Connection", &["fetchOne"]),
    ("Doctrine\\DBAL\\Result", &["fetchOne"]),
];

/// `(int) $query->getSingleScalarResult()` depends on the DQL and mapped fields.
/// Leave this unresolved if the method signature alone provides only mixed.
pub fn unresolved_result(index: &Index, class: &str, method: &str, result: ValueType) -> ValueType {
    if !matches!(result, ValueType::ExplicitMixed | ValueType::ImplicitMixed) {
        return result;
    }
    let Some((owner, _)) = index.method_owner(class, method) else {
        return result;
    };
    if QUERY_RESULTS.iter().any(|(class, methods)| {
        class.eq_ignore_ascii_case(&owner.name)
            && methods.iter().any(|name| name.eq_ignore_ascii_case(method))
    }) {
        ValueType::Mixed
    } else {
        result
    }
}

pub fn mapped_repository(attributes: &[Attribute], names: &Names) -> Option<String> {
    for attribute in attributes {
        let attribute_name = names.resolve(&attribute.name, UseKind::Class);
        if !attribute_name[0].eq_ignore_ascii_case("Doctrine\\ORM\\Mapping\\Entity") {
            continue;
        }
        let argument = attribute
            .arguments
            .iter()
            .find(|argument| {
                argument
                    .name
                    .as_ref()
                    .is_some_and(|name| name.name == "repositoryClass")
            })
            .or_else(|| {
                attribute
                    .arguments
                    .first()
                    .filter(|argument| argument.name.is_none())
            })?;
        if let ExpressionKind::ClassConstant {
            class,
            name: MemberName::Identifier(constant),
        } = &argument.value.kind
            && constant.name.eq_ignore_ascii_case("class")
            && let ExpressionKind::Name(class) = &class.kind
        {
            return names.resolve(class, UseKind::Class).into_iter().next();
        }
    }
    None
}

pub fn return_type(
    index: &Index,
    receiver: &str,
    method: &str,
    arguments: &[ValueType],
) -> Option<ValueType> {
    let method = if method.eq_ignore_ascii_case("getRepository") {
        "getRepository"
    } else if method.eq_ignore_ascii_case("getReference") || method.eq_ignore_ascii_case("find") {
        "entity"
    } else {
        return None;
    };
    let ValueType::ClassString(entity) = arguments.first()? else {
        return None;
    };
    if !index
        .ancestry(receiver)
        .iter()
        .any(|base| base.name == MANAGER)
    {
        return None;
    }
    if method == "entity" {
        return Some(entity.join(&ValueType::Null));
    }
    let mapped = entity
        .class_name()
        .and_then(|name| index.class(name))
        .and_then(|class| class.mapped_repository.as_ref());
    Some(if let Some(mapped) = mapped {
        ValueType::Class(mapped.clone())
    } else {
        ValueType::Generic(Arc::new(Generic {
            base: REPOSITORY.into(),
            arguments: vec![(**entity).clone()],
        }))
    })
}
