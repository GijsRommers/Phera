//! SORT has a distinct option grammar: BY/GET patterns, LIMIT, direction and STORE.
use super::*;

pub fn build(name: &str, a: &[Value], b: &mut Builder<'_>) -> bool {
    match name {
        "sort" | "sort_ro" => {
            b.key(&a[0]);
            b.shape = Shape::Values;
            if let Some(options) = array(a.get(1)) {
                for (key, value) in options.iter() {
                    let phera_extension_api::ArrayKey::String(key) = key else {
                        continue;
                    };
                    match key.to_string_lossy().to_lowercase().as_str() {
                        "by" => {
                            b.token("BY");
                            b.key(value);
                        }
                        "get" => {
                            if let Some(patterns) = array(Some(value)) {
                                for (_, pattern) in patterns.iter() {
                                    b.token("GET");
                                    b.arg(pattern);
                                }
                            } else {
                                b.token("GET");
                                b.arg(value);
                            }
                        }
                        "limit" => {
                            b.token("LIMIT");
                            b.args(std::slice::from_ref(value));
                        }
                        "sort" => b.arg(value),
                        "alpha" if phera_extension_api::value::to_bool(value) => b.token("ALPHA"),
                        "store" => {
                            b.token("STORE");
                            b.key(value);
                            b.shape = Shape::Raw;
                        }
                        _ => (),
                    }
                }
            }
        }
        _ => return false,
    }
    true
}
