//! Sorted-set scores return PHP doubles; WITHSCORES returns member => score maps.
use super::*;
use crate::connection::number;

pub async fn build(
    name: &str,
    c: &mut dyn Context,
    a: &[Value],
    b: &mut Builder<'_>,
) -> Result<bool, NativeError> {
    match name {
        "zadd" => {
            b.key(&a[0]);
            let offset = if let Some(options) = array(a.get(1)) {
                for (_, value) in options.iter() {
                    let flag = string(value).to_uppercase();
                    if flag == "INCR" {
                        b.shape = Shape::Float;
                    }
                    b.token(flag);
                }
                2
            } else {
                1
            };
            for pair in a[offset..].chunks(2) {
                b.arg(&pair[0]);
                if let Some(member) = pair.get(1) {
                    b.value(c, member).await?;
                }
            }
        }
        "zincrby" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.value(c, &a[2]).await?;
            b.shape = Shape::Float;
        }
        "zscore" | "zrank" | "zrevrank" | "zrem" | "zmscore" => {
            b.key(&a[0]);
            for value in &a[1..] {
                b.value(c, value).await?;
            }
            b.shape = match name {
                "zscore" => Shape::Float,
                "zmscore" => Shape::Floats,
                _ => Shape::Raw,
            };
        }
        "zcard" | "zcount" | "zlexcount" | "zremrangebylex" | "zremrangebyrank"
        | "zremrangebyscore" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "zrange" | "zrevrange" | "zrangebyscore" | "zrevrangebyscore" | "zrangestore" => {
            b.key(&a[0]);
            let offset = usize::from(name == "zrangestore");
            if offset > 0 {
                b.key(&a[1]);
            }
            b.arg(&a[offset + 1]);
            b.arg(&a[offset + 2]);
            b.shape = Shape::Values;
            let options = a.get(offset + 3);
            if options == Some(&Value::Bool(true)) {
                b.token("WITHSCORES");
                b.shape = Shape::Scores;
            } else if let Some(options) = array(options) {
                for (key, value) in options.iter() {
                    match key {
                        phera_extension_api::ArrayKey::Int(_) => {
                            let flag = string(value).to_uppercase();
                            if flag == "WITHSCORES" {
                                b.shape = Shape::Scores;
                            }
                            b.token(flag);
                        }
                        phera_extension_api::ArrayKey::Interned(key) => {
                            let flag = key.to_string_lossy().to_uppercase();
                            if flag == "WITHSCORES" {
                                if phera_extension_api::value::to_bool(value) {
                                    b.token(flag);
                                    b.shape = Shape::Scores;
                                }
                            } else if flag == "LIMIT" {
                                b.token(flag);
                                b.args(std::slice::from_ref(value));
                            } else if phera_extension_api::value::to_bool(value) {
                                b.token(flag);
                            }
                        }
                        phera_extension_api::ArrayKey::String(key) => {
                            let flag = key.to_string_lossy().to_uppercase();
                            if flag == "WITHSCORES" {
                                if phera_extension_api::value::to_bool(value) {
                                    b.token(flag);
                                    b.shape = Shape::Scores;
                                }
                            } else if flag == "LIMIT" {
                                b.token(flag);
                                b.args(std::slice::from_ref(value));
                            } else if phera_extension_api::value::to_bool(value) {
                                b.token(flag);
                            }
                        }
                    }
                }
            }
            if name == "zrangestore" {
                b.shape = Shape::Raw;
            }
        }
        "zrangebylex" | "zrevrangebylex" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.arg(&a[2]);
            if number(a.get(3), -1.0) >= 0.0 && number(a.get(4), -1.0) >= 0.0 {
                b.token("LIMIT");
                b.arg(&a[3]);
                b.arg(&a[4]);
            }
            b.shape = Shape::Values;
        }
        "zpopmin" | "zpopmax" => {
            b.key(&a[0]);
            if let Some(count) = a.get(1).filter(|v| !matches!(v, Value::Null)) {
                b.arg(count);
            }
            b.shape = Shape::Scores;
        }
        "zrandmember" => {
            b.key(&a[0]);
            b.shape = Shape::Value;
            if let Some(count) = option(a.get(1), "count") {
                b.arg(count);
                b.shape = Shape::Values;
            }
            if option(a.get(1), "withscores").is_some_and(phera_extension_api::value::to_bool) {
                b.token("WITHSCORES");
                b.shape = Shape::Scores;
            }
        }
        "zdiff" | "zdiffstore" | "zinter" | "zunion" | "zinterstore" | "zunionstore"
        | "zintercard" => {
            let offset = usize::from(name.ends_with("store"));
            if offset > 0 {
                b.key(&a[0]);
            }
            b.token(array(a.get(offset)).map_or(0, |a| a.len()));
            b.keys(&a[offset..offset + 1]);
            if name == "zintercard" {
                if number(a.get(1), -1.0) >= 0.0 {
                    b.token("LIMIT");
                    b.arg(&a[1]);
                }
            } else {
                b.shape = if offset > 0 {
                    Shape::Raw
                } else {
                    Shape::Values
                };
                if name.contains("inter") || name.contains("union") {
                    if let Some(weights) = array(a.get(offset + 1)) {
                        b.token("WEIGHTS");
                        for (_, weight) in weights.iter() {
                            b.arg(weight);
                        }
                    }
                    if offset > 0 {
                        if let Some(aggregate) =
                            a.get(offset + 2).filter(|v| !matches!(v, Value::Null))
                        {
                            b.token("AGGREGATE");
                            b.arg(aggregate);
                        }
                    } else {
                        aggregate_options(b, a.get(offset + 2));
                    }
                } else if offset == 0 {
                    aggregate_options(b, a.get(1));
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn aggregate_options(b: &mut Builder<'_>, value: Option<&Value>) {
    if let Some(aggregate) = option(value, "aggregate") {
        b.token("AGGREGATE");
        b.arg(aggregate);
    }
    if option(value, "withscores").is_some_and(phera_extension_api::value::to_bool) {
        b.token("WITHSCORES");
        b.shape = Shape::Scores;
    }
}
