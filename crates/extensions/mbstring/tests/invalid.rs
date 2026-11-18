//! Exhaust invalid UTF-8 byte pairs: `mb_scrub("\xff") === '?'`.
use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, PhpArray, Registry, Value};
#[test]
fn every_utf8_byte_pair() {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    let mut ctx = TestContext::default();
    let script = format!(
        r#"<?php
{}
for ($a=0; $a<256; $a++) {{
    for ($b=0; $b<256; $b++) {{
        $s=chr($a).chr($b);
        run('pair', fn() => [mb_strlen($s), mb_check_encoding($s),
                            mb_scrub($s), mb_strtoupper($s)]);
    }}
}}
"#,
        oracle::PHP_PREAMBLE
    );
    let php = oracle::run_php("mbstring-byte-pairs", &script).unwrap();
    let mut rust = Vec::new();
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            let mut values = PhpArray::new();
            for name in [
                "mb_strlen",
                "mb_check_encoding",
                "mb_scrub",
                "mb_strtoupper",
            ] {
                values
                    .push(
                        call(
                            &registry,
                            &mut ctx,
                            name,
                            vec![Value::from(vec![first, second])],
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            rust.push(oracle::encode_case(
                "pair",
                &Ok(Value::from(values)),
                &ctx.output,
                &ctx.diagnostics,
            ));
        }
    }
    oracle::compare("UTF-8 byte pairs (262144 calls)", &[], &php, &rust);
}
