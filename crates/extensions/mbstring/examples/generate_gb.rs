//! Pin PHP's GB18030-2000 differences from encoding_rs's newer WHATWG mappings.
use phera_extension_api::testing::oracle;
fn main() {
    let script = r#"<?php
for ($cp=0; $cp<=0x10FFFF; $cp++) {
    $s=mb_chr($cp, 'GB18030');
    if ($s!==false) echo $cp, ' ', bin2hex($s), "\n";
}
"#;
    let lines = oracle::run_php("mbstring-gb-patches", script).expect("PHP 8.5 required");
    let mut patches = Vec::new();
    for line in lines {
        let (cp, hex) = line.split_once(' ').unwrap();
        let cp: u32 = cp.parse().unwrap();
        let expected: Vec<u8> = hex
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let mut buffer = [0; 4];
        let (actual, _, error) = encoding_rs::GB18030.encode(c.encode_utf8(&mut buffer));
        let decoded = encoding_rs::GB18030.decode_without_bom_handling(&expected);
        if error || actual.as_ref() != expected || decoded.0 != c.to_string() {
            patches.extend_from_slice(&cp.to_le_bytes());
            patches.push(expected.len() as u8);
            patches.extend_from_slice(&expected);
            patches.extend(std::iter::repeat_n(0, 4 - expected.len()));
        }
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/gb18030-patches.bin"),
        &patches,
    )
    .unwrap();
    println!("Generated {} GB18030 corrections", patches.len() / 9);
}
