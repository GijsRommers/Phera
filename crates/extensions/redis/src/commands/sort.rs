//! SORT has a distinct option grammar: BY/GET patterns, LIMIT, direction and STORE.
use super::*;
use crate::connection::number;

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
        "sortasc" | "sortascalpha" | "sortdesc" | "sortdescalpha" => {
            b.command = redis::cmd("SORT");
            b.key(&a[0]);
            b.shape = Shape::Values;
            if let Some(pattern) = a.get(1).filter(|v| !matches!(v, Value::Null)) {
                b.token("BY");
                b.key(pattern);
            }
            if let Some(get) = a.get(2).filter(|v| !matches!(v, Value::Null)) {
                if let Some(patterns) = array(Some(get)) {
                    for (_, pattern) in patterns.iter() {
                        b.token("GET");
                        b.arg(pattern);
                    }
                } else {
                    b.token("GET");
                    b.arg(get);
                }
            }
            if number(a.get(3), -1.0) >= 0.0 && number(a.get(4), -1.0) >= 0.0 {
                b.token("LIMIT");
                b.arg(&a[3]);
                b.arg(&a[4]);
            }
            b.token(if name.starts_with("sortdesc") {
                "DESC"
            } else {
                "ASC"
            });
            if name.ends_with("alpha") {
                b.token("ALPHA");
            }
            if let Some(store) = a.get(5).filter(|v| !matches!(v, Value::Null)) {
                b.token("STORE");
                b.key(store);
                b.shape = Shape::Raw;
            }
        }
        _ => return false,
    }
    true
}
