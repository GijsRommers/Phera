//! Compare every scalar against real PHP, independently of the generated tables.
use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, PhpArray, Registry, Value};
#[test]
fn whole_unicode() {
    let mut registry = Registry::for_level(phera_extension_api::testing::oracle::test_level());
    Mbstring.register(&mut registry);
    let mut ctx = TestContext::default();
    let mut calls = 0;
    for plane in 0..=16 {
        let start = plane * 65536;
        let end = start + 65536;
        let script = format!(
            r#"<?php
{}
for ($cp = {start}; $cp < {end}; $cp++) {{
    $s = mb_chr($cp, 'UTF-8');
    if ($s === false) {{ run('cp', fn() => $s); continue; }}
    run('cp', function() use ($s, $cp) {{
        $v = [mb_strtoupper($s), mb_strtolower($s)];
        for ($mode = 0; $mode < 8; $mode++) $v[] = mb_convert_case($s, $mode);
        return [...$v, mb_strwidth($s), mb_ord($s), mb_chr($cp), mb_check_encoding($s)];
    }});
}}
"#,
            oracle::PHP_PREAMBLE
        );
        let php = oracle::run_php(&format!("mbstring-unicode-{plane}"), &script).unwrap();
        let mut rust = Vec::new();
        for cp in start..end {
            let Some(c) = char::from_u32(cp) else {
                let result = call(
                    &registry,
                    &mut ctx,
                    "mb_chr",
                    vec![Value::Int(i64::from(cp))],
                );
                rust.push(oracle::encode_case(
                    "cp",
                    &result,
                    &ctx.output,
                    &ctx.diagnostics,
                ));
                calls += 1;
                continue;
            };
            let s = Value::from(c.to_string());
            let mut values = PhpArray::new();
            for name in ["mb_strtoupper", "mb_strtolower"] {
                values
                    .push(call(&registry, &mut ctx, name, vec![s.clone()]).unwrap())
                    .unwrap();
                calls += 1;
            }
            for mode in 0..8 {
                values
                    .push(
                        call(
                            &registry,
                            &mut ctx,
                            "mb_convert_case",
                            vec![s.clone(), Value::Int(mode)],
                        )
                        .unwrap(),
                    )
                    .unwrap();
                calls += 1;
            }
            for name in ["mb_strwidth", "mb_ord", "mb_chr", "mb_check_encoding"] {
                let arg = if name == "mb_chr" {
                    Value::Int(i64::from(cp))
                } else {
                    s.clone()
                };
                values
                    .push(call(&registry, &mut ctx, name, vec![arg]).unwrap())
                    .unwrap();
                calls += 1;
            }
            rust.push(oracle::encode_case(
                "cp",
                &Ok(Value::from(values)),
                &ctx.output,
                &ctx.diagnostics,
            ));
        }
        oracle::compare(&format!("Unicode plane {plane}"), &[], &php, &rust);
    }
    println!("Whole Unicode: {calls} compared calls, zero mismatches");
}
