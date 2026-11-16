use super::*;
use phera_extension_api::value::to_php_string;
use std::rc::Rc;
pub(crate) fn register(r: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("search", ParamType::ArrayOrString),
        Param::new("replace", ParamType::ArrayOrString),
        Param::new("subject", ParamType::ArrayOrString),
        Param::new("count", ParamType::Mixed).by_ref().optional(),
    ];
    r.function(FunctionEntry::sync("str_replace", PARAMS, replace));
    r.function(FunctionEntry::sync("str_ireplace", PARAMS, ireplace));
}
fn replace(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    run(c, a, false)
}
fn ireplace(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    run(c, a, true)
}
fn strings(c: &mut dyn Context, v: &Value) -> Result<Vec<PhpString>, NativeError> {
    match v {
        Value::Array(arr) => arr
            .values()
            .map(|v| to_php_string(v, c.diagnostics()).map_err(Into::into))
            .collect(),
        _ => Ok(vec![to_php_string(v, c.diagnostics())?]),
    }
}
fn run(c: &mut dyn Context, a: &[Value], fold: bool) -> NativeResult {
    let name = if fold { "str_ireplace" } else { "str_replace" };
    if args::array(a, 0).is_none() && args::array(a, 1).is_some() {
        return Err(NativeError::type_error(format!(
            "{name}(): Argument #2 ($replace) must be of type string when argument #1 ($search) is a string"
        )));
    }
    let search = strings(c, a.first().unwrap_or(&Value::Null))?;
    let replacement = strings(c, a.get(1).unwrap_or(&Value::Null))?;
    let repeated = args::array(a, 1).is_none();
    let mut count = 0i64;
    let mut replace_one = |c: &mut dyn Context, v: &Value| -> NativeResult {
        let mut s = to_php_string(v, c.diagnostics())?.as_bytes().to_vec();
        for (i, n) in search.iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            let rep = replacement
                .get(if repeated { 0 } else { i })
                .map_or(&[][..], PhpString::as_bytes);
            let mut out = Vec::new();
            let mut start = 0;
            while let Some(tail) = s.get(start..) {
                let Some(pos) = tail.windows(n.len()).position(|w| {
                    if fold {
                        w.eq_ignore_ascii_case(n.as_bytes())
                    } else {
                        w == n.as_bytes()
                    }
                }) else {
                    break;
                };
                let len = out.len().saturating_add(pos).saturating_add(rep.len());
                if len > 2 * 1024 * 1024 * 1024 {
                    return Err(NativeError::error(
                        "String size exceeds the maximum allowed size",
                    ));
                }
                out.try_reserve(pos.saturating_add(rep.len()))
                    .map_err(|_| NativeError::error("Could not allocate string"))?;
                out.extend_from_slice(tail.get(..pos).unwrap_or_default());
                out.extend_from_slice(rep);
                start += pos + n.len();
                count = count.saturating_add(1);
            }
            let tail = s.get(start..).unwrap_or_default();
            if out.len().saturating_add(tail.len()) > 2 * 1024 * 1024 * 1024 {
                return Err(NativeError::error(
                    "String size exceeds the maximum allowed size",
                ));
            }
            out.try_reserve(tail.len())
                .map_err(|_| NativeError::error("Could not allocate string"))?;
            out.extend_from_slice(tail);
            s = out;
        }
        Ok(bytes(s))
    };
    let result = if let Some(arr) = args::array(a, 2) {
        let mut out = PhpArray::new();
        for (k, v) in arr.iter() {
            out.insert(k.clone(), replace_one(c, v)?);
        }
        Value::Array(Rc::new(out))
    } else {
        replace_one(c, a.get(2).unwrap_or(&Value::Null))?
    };
    if let Some(r) = args::reference(a, 3) {
        *r.try_borrow_mut()
            .map_err(|_| NativeError::error("Cannot modify borrowed reference"))? =
            Value::Int(count);
    }
    Ok(result)
}
