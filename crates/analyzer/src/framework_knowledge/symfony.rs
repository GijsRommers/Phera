//! The PHPStan Symfony extension gives `DenormalizerInterface::denormalize()`
//! a mixed return. Its configured normalizers determine the runtime result.

use crate::index::Index;

const DENORMALIZER: &str = "Symfony\\Component\\Serializer\\Normalizer\\DenormalizerInterface";

pub fn unresolved_result(index: &Index, class: &str, method: &str) -> bool {
    method.eq_ignore_ascii_case("denormalize")
        && index
            .method_owner(class, method)
            .is_some_and(|(owner, _)| owner.name.eq_ignore_ascii_case(DENORMALIZER))
}
