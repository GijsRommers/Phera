//! `list<User>` fits an array parameter, and PHP can widen `int` to `float`.
//! Use explicit requirements for unions and intersections. Unknown facts stay uncertain.

use super::*;
use crate::index::Index;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compatibility {
    Accepts,
    Rejects,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePolicy {
    Documentation,
    Strict,
    Coercive,
}

enum Requirement {
    All,
    Any,
}

enum Step<'a> {
    Compare(&'a ValueType, &'a ValueType),
    Reduce(usize, Requirement),
}

impl ValueType {
    pub fn accepts(&self, given: &Self, index: &Index, policy: TypePolicy) -> Compatibility {
        use Compatibility::*;
        let mut pending = vec![Step::Compare(self, given)];
        let mut results = Vec::new();
        while let Some(step) = pending.pop() {
            match step {
                Step::Reduce(count, requirement) => {
                    let start = results.len().saturating_sub(count);
                    let mut items = results.drain(start..);
                    let (decisive, fallback) = match requirement {
                        Requirement::All => (Rejects, Accepts),
                        Requirement::Any => (Accepts, Rejects),
                    };
                    let mut result = fallback;
                    for item in &mut items {
                        if item == decisive {
                            result = decisive;
                            break;
                        }
                        if item == Uncertain {
                            result = Uncertain;
                        }
                    }
                    drop(items);
                    results.push(result);
                }
                Step::Compare(expected, given) => {
                    if matches!(expected, Self::Mixed)
                        || matches!(given, Self::Never)
                        || expected == given
                    {
                        results.push(Accepts);
                        continue;
                    }
                    match (expected, given) {
                        (_, Self::Mixed) => results.push(Uncertain),
                        (_, Self::Class(name))
                            if index.class(name).is_none()
                                && let Some(constant) = index.doc_global_constant(name)
                                && !matches!(constant, Self::Class(_)) =>
                        {
                            pending.push(Step::Compare(expected, constant));
                        }
                        (_, Self::Union(types)) => {
                            pending.push(Step::Reduce(types.len(), Requirement::All));
                            pending.extend(types.iter().map(|ty| Step::Compare(expected, ty)));
                        }
                        (_, Self::Template(template)) => {
                            pending.push(Step::Compare(expected, &template.bound))
                        }
                        (_, Self::Conditional(conditional)) => {
                            pending.push(Step::Reduce(2, Requirement::All));
                            pending.push(Step::Compare(expected, &conditional.yes));
                            pending.push(Step::Compare(expected, &conditional.no));
                        }
                        (Self::Union(types), _) => {
                            pending.push(Step::Reduce(types.len(), Requirement::Any));
                            pending.extend(types.iter().map(|ty| Step::Compare(ty, given)));
                        }
                        (Self::Intersection(types), _) => {
                            pending.push(Step::Reduce(types.len(), Requirement::All));
                            pending.extend(types.iter().map(|ty| Step::Compare(ty, given)));
                        }
                        (_, Self::Intersection(types)) => {
                            pending.push(Step::Reduce(types.len(), Requirement::Any));
                            pending.extend(types.iter().map(|ty| Step::Compare(expected, ty)));
                        }
                        (Self::Template(template), _) => {
                            pending.push(Step::Compare(&template.bound, given))
                        }
                        (Self::ClassString(expected), Self::ClassString(given)) => {
                            pending.push(Step::Compare(expected, given));
                        }
                        (Self::Collection(expected), Self::Collection(given))
                            if collection_accepts(expected, given) =>
                        {
                            pending.push(Step::Reduce(2, Requirement::All));
                            pending.push(Step::Compare(&expected.key, &given.key));
                            pending.push(Step::Compare(&expected.value, &given.value));
                        }
                        (_, _) => results.push(atomic(expected, given, index, policy)),
                    }
                }
            }
        }
        results.pop().unwrap_or(Uncertain)
    }
}

fn collection_accepts(expected: &Collection, given: &Collection) -> bool {
    (!expected.non_empty || given.non_empty)
        && (expected.kind == given.kind
            || expected.kind == CollectionKind::Iterable
            || expected.kind == CollectionKind::Array && given.kind == CollectionKind::List)
}

fn atomic(
    expected: &ValueType,
    given: &ValueType,
    index: &Index,
    policy: TypePolicy,
) -> Compatibility {
    use Compatibility::*;
    use ValueType::*;
    let accepted = match (expected, given) {
        (Object, Class(_) | Generic(_) | Callable(_) | This) => true,
        (Object, Shape(shape)) => shape.kind == ShapeKind::Object,
        (Int, IntLiteral(_) | IntRange(..)) => true,
        (IntRange(min, max), IntLiteral(value)) => (min..=max).contains(&value),
        (IntRange(min, max), IntRange(start, end)) => min <= start && end <= max,
        (Bool, BoolLiteral(_)) => true,
        (String, StringLiteral(_) | StringRefinement(_) | ClassString(_)) => true,
        (StringRefinement(kind), StringLiteral(bytes)) => string_accepts(*kind, bytes),
        (StringRefinement(StringKind::NonEmpty), ClassString(_)) => true,
        (StringRefinement(StringKind::NonEmpty), StringRefinement(StringKind::Truthy)) => true,
        (UnknownCallable, Callable(_)) => true,
        (Class(name), Callable(_)) if name.eq_ignore_ascii_case("Closure") => true,
        (Class(name), This) if matches!(name.as_str(), "self" | "static") => true,
        (Class(expected), Class(given))
            if ["self", "static"].contains(&expected.as_str())
                && ["self", "static"].contains(&given.as_str()) =>
        {
            true
        }
        (UnknownCallable, Class(name)) => {
            name.eq_ignore_ascii_case("Closure") || index.method(name, "__invoke").is_some()
        }
        (UnknownCallable, Generic(generic)) => index.method(&generic.base, "__invoke").is_some(),
        (UnknownCallable, String | StringLiteral(_) | Array | Collection(_) | Shape(_)) => {
            return Uncertain;
        }
        (Array, Collection(collection)) => collection.kind != CollectionKind::Iterable,
        (Iterable, Array | Collection(_)) => true,
        (Array | Iterable, Shape(shape)) => shape.kind != ShapeKind::Object,
        (Collection(collection), Array | Shape(_)) if collection.kind != CollectionKind::List => {
            return Uncertain;
        }
        (Iterable, Class(class)) => return nominal("Traversable", class, index),
        (expected, given) if expected.class_name().is_some() && given.class_name().is_some() => {
            let relation = nominal(
                expected.class_name().unwrap_or_default(),
                given.class_name().unwrap_or_default(),
                index,
            );
            return if relation == Accepts && matches!(expected, Generic(_)) {
                Uncertain
            } else {
                relation
            };
        }
        (Projection(_) | TypeConstant(_), _) | (_, Projection(_) | TypeConstant(_)) => {
            return Uncertain;
        }
        (Float, Int | IntLiteral(_) | IntRange(..)) if policy != TypePolicy::Documentation => true,
        (
            Bool,
            Int | IntLiteral(_) | IntRange(..) | Float | String | StringLiteral(_)
            | StringRefinement(_),
        ) if policy == TypePolicy::Coercive => true,
        (String, Int | IntLiteral(_) | IntRange(..) | Float | Bool | BoolLiteral(_))
            if policy == TypePolicy::Coercive =>
        {
            true
        }
        (String, Class(class)) if policy == TypePolicy::Coercive => {
            index.method(class, "__toString").is_some()
        }
        (Int | Float, StringLiteral(literal)) if policy == TypePolicy::Coercive => numeric(literal),
        (Int | Float, String | StringRefinement(_)) if policy == TypePolicy::Coercive => {
            return Uncertain;
        }
        (Int | Float, Bool | BoolLiteral(_)) | (Int, Float) if policy == TypePolicy::Coercive => {
            true
        }
        _ => false,
    };
    if accepted { Accepts } else { Rejects }
}

fn nominal(expected: &str, given: &str, index: &Index) -> Compatibility {
    use Compatibility::*;
    if expected.eq_ignore_ascii_case(given)
        || index
            .ancestry(given)
            .iter()
            .any(|class| class.name.eq_ignore_ascii_case(expected))
    {
        Accepts
    } else if index.class(expected).is_none()
        || index.class(given).is_none()
        || !index.complete(given)
        || ["self", "static", "parent"].contains(&expected)
        || ["self", "static", "parent"].contains(&given)
    {
        Uncertain
    } else {
        Rejects
    }
}

fn numeric(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.trim().parse::<f64>().ok())
        .is_some_and(f64::is_finite)
}

fn string_accepts(kind: StringKind, bytes: &[u8]) -> bool {
    match kind {
        StringKind::NonEmpty => !bytes.is_empty(),
        StringKind::Truthy => !bytes.is_empty() && bytes != b"0",
        StringKind::Numeric => numeric(bytes),
        StringKind::Literal => true,
        StringKind::Lowercase => !bytes.iter().any(u8::is_ascii_uppercase),
        StringKind::Uppercase => !bytes.iter().any(u8::is_ascii_lowercase),
    }
}
