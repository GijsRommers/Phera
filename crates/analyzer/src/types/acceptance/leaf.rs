//! Compare leaf types such as `bool`, `Stringable`, and `User`.
//! Nominal types consult the declaration index and defer incomplete ancestry.

use super::*;

pub(super) fn atomic(
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
        (Shape(shape), ty) if shape.kind == ShapeKind::Object && ty.class_name().is_some() => {
            let class = ty.class_name().unwrap_or_default();
            // A database row represented by stdClass can acquire its fields
            // dynamically. An ordinary indexed class still has to declare them.
            if index.class(class).is_none()
                || class.eq_ignore_ascii_case("stdClass")
                || index.open_properties(class)
                || !index.complete(class)
            {
                return Uncertain;
            }
            if shape.fields.iter().any(|field| {
                !field.optional
                    && matches!(&field.key,
                ShapeKey::String(name) if index.property(class, name).is_none())
            }) {
                return Rejects;
            }
            return Uncertain;
        }
        (Int, IntLiteral(_) | IntRange(..)) => true,
        (IntRange(min, max), IntLiteral(value)) => (min..=max).contains(&value),
        (IntRange(min, max), IntRange(start, end)) => min <= start && end <= max,
        (Bool, BoolLiteral(_)) => true,
        (String, StringLiteral(_) | StringRefinement(_) | ClassString(_)) => true,
        (StringRefinement(kind), StringLiteral(bytes)) => string_accepts(*kind, bytes),
        (StringRefinement(StringKind::NonEmpty), ClassString(_)) => true,
        (ClassString(bound), StringLiteral(value)) => {
            let Some(class) = std::str::from_utf8(value)
                .ok()
                .and_then(|name| index.class(name.trim_start_matches('\\')))
            else {
                return Rejects;
            };
            if **bound == Object {
                true
            } else if let Some(bound) = bound.class_name() {
                return nominal(bound, &class.name, index);
            } else {
                return Uncertain;
            }
        }
        (StringRefinement(StringKind::NonEmpty), StringRefinement(StringKind::Truthy)) => true,
        (UnknownCallable, Callable(_)) => true,
        (Class(name), Callable(_)) if name.eq_ignore_ascii_case("Closure") => true,
        (Class(name), UnknownCallable) | (Callable(_), Class(name))
            if name.eq_ignore_ascii_case("Closure") && policy != TypePolicy::Documentation =>
        {
            return Uncertain;
        }
        (Callable(_), UnknownCallable) => return Uncertain,
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
        (Collection(_), Array) | (Shape(_), Array | Collection(_)) => {
            return Uncertain;
        }
        (Iterable, Class(class)) => return nominal("Traversable", class, index),
        (Iterable, Generic(generic)) => return nominal("Traversable", &generic.base, index),
        (Collection(collection), ty)
            if collection.kind == CollectionKind::Iterable && ty.class_name().is_some() =>
        {
            return match nominal("Traversable", ty.class_name().unwrap_or_default(), index) {
                Accepts => Uncertain,
                relation => relation,
            };
        }
        (expected, given) if expected.class_name().is_some() && given.class_name().is_some() => {
            let expected_name = expected.class_name().unwrap_or_default();
            let given_name = given.class_name().unwrap_or_default();
            let relation = nominal(expected_name, given_name, index);
            // A generic return such as Result<T> has unknown template
            // arguments when it comes through one of Result's interfaces.
            let may_be_generic = matches!(expected, Generic(_))
                && (relation == Accepts
                    || policy != TypePolicy::Documentation
                        && index
                            .class(given_name)
                            .is_some_and(|class| class.kind == phera_ast::ClassKind::Interface)
                        && nominal(given_name, expected_name, index) == Accepts);
            return if may_be_generic { Uncertain } else { relation };
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
        (String, Class(class))
            if matches!(policy, TypePolicy::Coercive | TypePolicy::WeakValue) =>
        {
            index.method(class, "__toString").is_some()
        }
        (Class(_) | Generic(_), Object)
        | (ClassString(_) | StringRefinement(_) | StringLiteral(_), String)
        | (IntRange(..), Int)
            if policy != TypePolicy::Documentation =>
        {
            return Uncertain;
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
