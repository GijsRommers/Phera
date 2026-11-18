//! Generate kana mappings observed from `mb_convert_kana('ｶﾞ', 'KV') === 'ガ'`.
use phera_extension_api::testing::oracle;
fn main() {
    let script = r#"<?php
foreach (str_split('ARNSKHMCarnskhmc') as $flag) {
    if ($flag === ' ') continue;
    for ($cp=0; $cp<=0xFFFF; $cp++) {
        if ($cp>=0xD800 && $cp<=0xDFFF) continue;
        $s=mb_chr($cp);
        $out=mb_convert_kana($s, $flag);
        if ($s!==$out) echo $flag, ' ', $cp, ' ',
            implode(',', array_map('mb_ord', mb_str_split($out))), "\n";
    }
}
foreach (['K','H'] as $flag) {
    for ($cp=0xFF61; $cp<=0xFF9F; $cp++) {
        foreach ([0xFF9E,0xFF9F] as $next) {
            $out=mb_convert_kana(mb_chr($cp).mb_chr($next), $flag.'V');
            if (mb_strlen($out)===1) echo $flag, ' ', $cp, ':', $next, ' ', mb_ord($out), "\n";
        }
    }
}
"#;
    let lines = oracle::run_php("mbstring-kana-data", script).expect("PHP 8.5 required");
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/kana.txt"),
        lines.join("\n"),
    )
    .unwrap();
}
