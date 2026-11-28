//! Sound conversions between PHP's runtime-enforced [`Type`] declarations and
//! closed-world [`SoundType`] values.
//!
//! Only runtime-enforced semantics are modeled here — never PHPDoc. Every helper
//! strictly over-approximates what PHP 8.5 allows at run time, and `is_exact_subtype`
//! returns `true` only when PHP's runtime type check succeeds with zero coercion.

use phera_bytecode::facts::{ClassBound, ClassFlags, ClassRef, SoundMask, SoundType};
use phera_bytecode::{CompiledUnit, Type, TypeId, TypeMask};

use super::hierarchy::Hierarchy;

/// Masks representing all non-undefined PHP runtime values (`mixed`).
pub(crate) const MIXED_MASK: SoundMask = SoundMask(
    SoundMask::NULL.0
        | SoundMask::BOOL.0
        | SoundMask::INT.0
        | SoundMask::FLOAT.0
        | SoundMask::STRING.0
        | SoundMask::ARRAY.0
        | SoundMask::OBJECT.0
        | SoundMask::RESOURCE.0,
);

/// Converts a compiled PHP [`Type`] declaration (from a parameter, return, or
/// property declaration) into a sound over-approximation of the values PHP's
/// runtime type check permits.
///
/// `current_class` is the declaring class scope (used for `self`, `parent`, and
/// `static`). When `current_class` is `None` (free function, closure, or trait
/// body whose consumer varies), class-relative types widen to `object`.
pub(crate) fn sound_type_of_declared(
    unit: &CompiledUnit,
    ty_id: TypeId,
    hierarchy: &Hierarchy,
    current_class: Option<ClassRef>,
) -> SoundType {
    let Some(ty) = unit.types.get(ty_id.index()) else {
        return SoundType::UNKNOWN;
    };
    sound_type_of_type(unit, ty, hierarchy, current_class)
}

pub(crate) fn sound_type_of_type(
    unit: &CompiledUnit,
    ty: &Type,
    hierarchy: &Hierarchy,
    current_class: Option<ClassRef>,
) -> SoundType {
    // `mixed` accepts every PHP value including `resource` (but never `UNDEF`).
    if ty.display == "mixed" {
        return SoundType {
            mask: MIXED_MASK,
            class: ClassBound::Any,
        };
    }

    let mut result = SoundType::NEVER;
    let m = ty.mask;
    if m.contains(TypeMask::NULL) {
        result = result.union(SoundType::NULL);
    }
    if m.contains(TypeMask::FALSE) {
        result = result.union(SoundType {
            mask: SoundMask::FALSE,
            class: ClassBound::Any,
        });
    }
    if m.contains(TypeMask::TRUE) {
        result = result.union(SoundType {
            mask: SoundMask::TRUE,
            class: ClassBound::Any,
        });
    }
    if m.contains(TypeMask::INT) {
        result = result.union(SoundType::INT);
    }
    if m.contains(TypeMask::FLOAT) {
        result = result.union(SoundType::FLOAT);
    }
    if m.contains(TypeMask::STRING) {
        result = result.union(SoundType::STRING);
    }
    if m.contains(TypeMask::ARRAY) {
        result = result.union(SoundType::ARRAY);
    }
    if m.contains(TypeMask::OBJECT) {
        result = result.union(SoundType {
            mask: SoundMask::OBJECT,
            class: ClassBound::Any,
        });
    }
    if m.contains(TypeMask::CALLABLE) {
        // PHP callables may be strings, arrays, Closures, or invokable objects.
        result = result.union(SoundType {
            mask: SoundMask::STRING
                .union(SoundMask::ARRAY)
                .union(SoundMask::OBJECT),
            class: ClassBound::Any,
        });
    }
    if m.contains(TypeMask::ITERABLE) {
        // PHP iterables are `array|Traversable`.
        result = result.union(SoundType {
            mask: SoundMask::ARRAY.union(SoundMask::OBJECT),
            class: ClassBound::Any,
        });
    }
    if m.contains(TypeMask::VOID) {
        // A `void` function returns `null` at runtime.
        result = result.union(SoundType::NULL);
    }
    if m.contains(TypeMask::STATIC) {
        result = result.union(resolve_class_bound("static", hierarchy, current_class));
    }

    if !ty.classes.is_empty() {
        if ty.intersection {
            // For pure intersections (`A&B`) without a union, any member bound is
            // a valid sound over-approximation. For DNF unions (`(A&B)|C`), widen
            // the object component to `ClassBound::Any` to stay strictly sound.
            if ty.display.contains('|') {
                result = result.union(SoundType {
                    mask: SoundMask::OBJECT,
                    class: ClassBound::Any,
                });
            } else {
                // Pick the tightest single class bound in the intersection.
                let mut chosen = SoundType {
                    mask: SoundMask::OBJECT,
                    class: ClassBound::Any,
                };
                for &name_id in &ty.classes {
                    if let Some(raw) = unit.names.get(name_id.index()) {
                        let clean = raw.trim_matches(['(', ')']);
                        let candidate = resolve_class_bound(clean, hierarchy, current_class);
                        if matches!(candidate.class, ClassBound::Exact(_)) {
                            chosen = candidate;
                            break;
                        }
                        if matches!(candidate.class, ClassBound::Within(_))
                            && matches!(chosen.class, ClassBound::Any)
                        {
                            chosen = candidate;
                        }
                    }
                }
                result = result.union(chosen);
            }
        } else {
            for &name_id in &ty.classes {
                let bound = unit
                    .names
                    .get(name_id.index())
                    .map(|name| resolve_class_bound(name, hierarchy, current_class))
                    .unwrap_or(SoundType {
                        mask: SoundMask::OBJECT,
                        class: ClassBound::Any,
                    });
                result = result.union(bound);
            }
        }
    }

    // `never` has mask NONE and no classes; return `SoundType::NEVER`.
    if m.contains(TypeMask::NEVER) && result == SoundType::NEVER {
        return SoundType::NEVER;
    }

    result
}

/// Resolves a lowercased PHP class type name (`self`, `parent`, `static`, or FQCN)
/// into a sound `OBJECT` [`SoundType`].
pub(crate) fn resolve_class_bound(
    lowercased: &str,
    hierarchy: &Hierarchy,
    current_class: Option<ClassRef>,
) -> SoundType {
    let target = match lowercased {
        "self" | "static" => current_class,
        "parent" => current_class.and_then(|c| hierarchy.classes.get(c.0 as usize)?.parent),
        name if name.starts_with('\0') => None,
        name => hierarchy.class_ref(name.trim_start_matches('\\')),
    };
    match target {
        Some(cref) => class_sound_type(cref, hierarchy),
        None => SoundType {
            mask: SoundMask::OBJECT,
            class: ClassBound::Any,
        },
    }
}

/// Returns `Exact(c)` if `c` is concrete and `EFFECTIVELY_FINAL` (no subclasses
/// can exist), otherwise `Within(c)`.
pub(crate) fn class_sound_type(class: ClassRef, hierarchy: &Hierarchy) -> SoundType {
    let is_closed = hierarchy
        .classes
        .get(class.0 as usize)
        .is_some_and(|layout| layout.flags.contains(ClassFlags::EFFECTIVELY_FINAL));
    if is_closed {
        SoundType::exact_class(class)
    } else {
        SoundType::within_class(class)
    }
}

/// Whether `actual` is a strict, zero-coercion subtype of the declared PHP type
/// `expected`.
///
/// If this returns `true`, PHP's runtime type check is guaranteed to accept any
/// value conforming to `actual` without throwing `TypeError` and without
/// mutating/coercing the value (for example, `int` is NOT an exact subtype of
/// `float` because PHP converts `int` to `float`).
pub(crate) fn is_exact_subtype(
    actual: SoundType,
    unit: &CompiledUnit,
    expected_id: TypeId,
    hierarchy: &Hierarchy,
    expected_class_scope: Option<ClassRef>,
) -> bool {
    let Some(expected) = unit.types.get(expected_id.index()) else {
        return false;
    };
    if actual.mask.intersects(SoundMask::UNDEF) {
        return false;
    }
    // Unreachable code (`NEVER`) vacuously satisfies any expected type.
    if actual.mask == SoundMask::NONE {
        return true;
    }
    if expected.display == "mixed" {
        return true;
    }
    // `never` rejects every runtime value.
    if expected.mask.contains(TypeMask::NEVER) {
        return false;
    }
    // `void` only accepts `ReturnNull` / implicit null; reject non-null values.
    if expected.mask.contains(TypeMask::VOID) {
        return actual.mask == SoundMask::NULL;
    }
    // Never accept `resource` for anything other than `mixed`.
    if actual.mask.intersects(SoundMask::RESOURCE) {
        return false;
    }
    // Be conservative on complex intersections / DNF unless `object` bit alone covers it.
    if expected.intersection && actual.mask.intersects(SoundMask::OBJECT) {
        return false;
    }

    // Check every non-object bit in `actual.mask` against `expected.mask`.
    let non_obj_actual = actual.mask.0 & !SoundMask::OBJECT.0;
    let mut allowed_non_obj: u16 = 0;
    if expected.mask.contains(TypeMask::NULL) {
        allowed_non_obj |= SoundMask::NULL.0;
    }
    if expected.mask.contains(TypeMask::FALSE) {
        allowed_non_obj |= SoundMask::FALSE.0;
    }
    if expected.mask.contains(TypeMask::TRUE) {
        allowed_non_obj |= SoundMask::TRUE.0;
    }
    if expected.mask.contains(TypeMask::INT) {
        allowed_non_obj |= SoundMask::INT.0;
    }
    if expected.mask.contains(TypeMask::FLOAT) {
        allowed_non_obj |= SoundMask::FLOAT.0;
    }
    if expected.mask.contains(TypeMask::STRING) {
        allowed_non_obj |= SoundMask::STRING.0;
    }
    if expected.mask.contains(TypeMask::ARRAY) || expected.mask.contains(TypeMask::ITERABLE) {
        allowed_non_obj |= SoundMask::ARRAY.0;
    }
    if (non_obj_actual & !allowed_non_obj) != 0 {
        return false;
    }

    // Now check the `OBJECT` component if `actual` may be an object.
    if actual.mask.intersects(SoundMask::OBJECT) {
        if expected.mask.contains(TypeMask::OBJECT) {
            return true;
        }
        let actual_class = match actual.class {
            ClassBound::Exact(c) | ClassBound::Within(c) => c,
            ClassBound::Any => return false,
        };
        // Check `iterable` -> `Traversable`.
        if expected.mask.contains(TypeMask::ITERABLE)
            && hierarchy
                .class_ref("traversable")
                .is_some_and(|trav| is_class_subtype(actual_class, trav, hierarchy))
        {
            return true;
        }
        // Check `static` only when `expected_class_scope` is EFFECTIVELY_FINAL (exact).
        if expected.mask.contains(TypeMask::STATIC)
            && let Some(scope) = expected_class_scope
            && hierarchy
                .classes
                .get(scope.0 as usize)
                .is_some_and(|l| l.flags.contains(ClassFlags::EFFECTIVELY_FINAL))
            && actual_class == scope
        {
            return true;
        }
        // Check each allowed class in `expected.classes`.
        let matched = expected.classes.iter().any(|&name_id| {
            let Some(raw) = unit.names.get(name_id.index()) else {
                return false;
            };
            let target = match raw.as_str() {
                "self" => expected_class_scope,
                "parent" => {
                    expected_class_scope.and_then(|c| hierarchy.classes.get(c.0 as usize)?.parent)
                }
                "static" => expected_class_scope.filter(|&scope| {
                    hierarchy
                        .classes
                        .get(scope.0 as usize)
                        .is_some_and(|l| l.flags.contains(ClassFlags::EFFECTIVELY_FINAL))
                }),
                name if name.starts_with('\0') => None,
                name => hierarchy.class_ref(name.trim_start_matches('\\')),
            };
            target.is_some_and(|exp_ref| is_class_subtype(actual_class, exp_ref, hierarchy))
        });
        if !matched {
            return false;
        }
    }

    true
}

/// True when `sub` is equal to `sup` or has `sup` in its ancestor list.
pub(crate) fn is_class_subtype(sub: ClassRef, sup: ClassRef, hierarchy: &Hierarchy) -> bool {
    if sub == sup {
        return true;
    }
    hierarchy
        .classes
        .get(sub.0 as usize)
        .is_some_and(|layout| layout.ancestors.binary_search(&sup).is_ok())
}

/// Sound join of two `SoundType` values using the class hierarchy to preserve a
/// common ancestor when both sides are objects (if one is a subtype of the other).
pub(crate) fn join_sound_types(a: SoundType, b: SoundType, hierarchy: &Hierarchy) -> SoundType {
    if a == b {
        return a;
    }
    let mask = a.mask.union(b.mask);
    let class = match (
        a.mask.intersects(SoundMask::OBJECT),
        b.mask.intersects(SoundMask::OBJECT),
    ) {
        (false, false) => ClassBound::Any,
        (true, false) => a.class,
        (false, true) => b.class,
        (true, true) => match (a.class, b.class) {
            (x, y) if x == y => x,
            (ClassBound::Exact(x), ClassBound::Within(y))
            | (ClassBound::Within(y), ClassBound::Exact(x))
                if is_class_subtype(x, y, hierarchy) =>
            {
                ClassBound::Within(y)
            }
            (ClassBound::Exact(x), ClassBound::Exact(y))
            | (ClassBound::Within(x), ClassBound::Within(y)) => {
                if is_class_subtype(x, y, hierarchy) {
                    ClassBound::Within(y)
                } else if is_class_subtype(y, x, hierarchy) {
                    ClassBound::Within(x)
                } else {
                    ClassBound::Any
                }
            }
            _ => ClassBound::Any,
        },
    };
    SoundType { mask, class }
}
