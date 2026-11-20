//! The PHPStan Symfony extension gives `DenormalizerInterface::denormalize()`
//! a mixed return. Its configured normalizers determine the runtime result.

use crate::{
    index::Index,
    types::{Compatibility, TypePolicy, ValueType},
};
use phera_ast::Arguments;

const DENORMALIZER: &str = "Symfony\\Component\\Serializer\\Normalizer\\DenormalizerInterface";
const DEFAULTING_BAGS: &[&str] = &[
    "Symfony\\Component\\HttpFoundation\\InputBag",
    "Symfony\\Component\\HttpFoundation\\HeaderBag",
];

fn unresolved_result(index: &Index, class: &str, method: &str) -> bool {
    method.eq_ignore_ascii_case("denormalize")
        && index
            .method_owner(class, method)
            .is_some_and(|(owner, _)| owner.name.eq_ignore_ascii_case(DENORMALIZER))
}

/// `$request->headers->get('Accept', 'text/plain')` excludes null with the Symfony extension.
pub fn return_type(
    index: &Index,
    class: &str,
    method: &str,
    arguments: &Arguments,
    types: &[ValueType],
    result: ValueType,
) -> ValueType {
    if unresolved_result(index, class, method) {
        return ValueType::Mixed;
    }
    if !method.eq_ignore_ascii_case("get")
        || !index.method_owner(class, method).is_some_and(|(owner, _)| {
            DEFAULTING_BAGS
                .iter()
                .any(|name| owner.name.eq_ignore_ascii_case(name))
        })
    {
        return result;
    }
    let Arguments::List(arguments) = arguments else {
        return result;
    };
    if arguments.iter().any(|argument| argument.spread) {
        return result;
    }
    let default = arguments
        .iter()
        .zip(types)
        .enumerate()
        .find_map(|(position, (argument, ty))| {
            argument
                .name
                .as_ref()
                .map_or(position == 1, |name| name.name == "default")
                .then_some(ty)
        });
    if default.is_some_and(|ty| {
        ty.accepts(&ValueType::Null, index, TypePolicy::Documentation) == Compatibility::Rejects
    }) {
        result.without_null()
    } else {
        result
    }
}

pub fn has_internal_polyfill(class: &str) -> bool {
    // PHPStan can resolve the polyfill's internal parent even with ext/intl present.
    class.eq_ignore_ascii_case("IntlDateFormatter")
}
