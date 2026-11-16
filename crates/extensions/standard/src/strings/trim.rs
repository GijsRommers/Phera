use super::*;
const PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String).optional(),
];
pub(crate) fn register(r: &mut Registry) {
    for (n, f) in [
        ("trim", trim as SyncFn),
        ("ltrim", left),
        ("rtrim", right),
        ("chop", chop),
    ] {
        r.function(FunctionEntry::sync(n, PARAMS, f));
    }
}
fn mask(c: &mut dyn Context, s: &[u8], name: &str) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut i = 0;
    while let Some(&b) = s.get(i) {
        if let Some(t) = s.get(i..i.saturating_add(4))
            && t.get(1) == Some(&b'.')
            && t.get(2) == Some(&b'.')
            && t.get(3).is_some_and(|e| *e >= b)
        {
            if let Some(&end) = t.get(3) {
                for v in b..=end {
                    mask[usize::from(v)] = true;
                }
            }
            i += 4;
            continue;
        }
        if b == b'.' && s.get(i + 1) == Some(&b'.') {
            let reason = if i == 0 {
                ", no character to the left of '..'"
            } else if i + 2 >= s.len() {
                ", no character to the right of '..'"
            } else if s.get(i - 1) > s.get(i + 2) {
                ", '..'-range needs to be incrementing"
            } else {
                ""
            };
            c.warning(format!("{name}(): Invalid '..'-range{reason}"));
        } else {
            mask[usize::from(b)] = true;
        }
        i += 1;
    }
    mask
}
fn run(c: &mut dyn Context, a: &[Value], name: &str, l: bool, r: bool) -> NativeResult {
    let s = args::bytes(a, 0);
    let chars = if args::given(a, 1) {
        args::bytes(a, 1)
    } else {
        b" \n\r\t\x0b\0"
    };
    let m = mask(c, chars, name);
    let mut out = s;
    if l {
        while out.first().is_some_and(|b| m[usize::from(*b)]) {
            out = out.get(1..).unwrap_or_default();
        }
    }
    if r {
        while out.last().is_some_and(|b| m[usize::from(*b)]) {
            out = out.get(..out.len() - 1).unwrap_or_default();
        }
    }
    Ok(bytes(out.to_vec()))
}
fn trim(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    run(c, a, "trim", true, true)
}
fn left(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    run(c, a, "ltrim", true, false)
}
fn right(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    run(c, a, "rtrim", false, true)
}

fn chop(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    run(c, a, "chop", false, true)
}
