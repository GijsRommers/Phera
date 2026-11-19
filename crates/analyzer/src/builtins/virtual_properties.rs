//! `$interval->days` and `$case->name` are provided by PHP's runtime.
//! Native stub declarations omit fields implemented by object handlers or enum cases.

use crate::{
    index::{Index, PropertyInfo},
    types::ValueType,
};

pub fn add(index: &mut Index) {
    use ValueType as T;
    for (class, names, ty) in [
        ("unitenum", &["name"][..], T::String),
        ("backedenum", &["value"][..], T::Int.join(&T::String)),
        (
            "dateinterval",
            &["y", "m", "d", "h", "i", "s", "invert"][..],
            T::Int,
        ),
        ("dateinterval", &["f"][..], T::Float),
        (
            "dateinterval",
            &["days"][..],
            T::Int.join(&T::BoolLiteral(false)),
        ),
    ] {
        if let Some(info) = index.classes.get_mut(class) {
            for name in names {
                info.properties
                    .entry((*name).into())
                    .or_insert_with(|| PropertyInfo {
                        ty: ty.clone(),
                        documented_type: None,
                        modifiers: Default::default(),
                        location: info.location.clone(),
                    });
            }
        }
    }
}
