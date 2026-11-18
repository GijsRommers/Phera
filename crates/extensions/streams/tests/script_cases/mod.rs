//! Independent scripts generated from a table of modes and CSV dialects.

use std::fs;
use std::path::{Path, PathBuf};

pub fn generate(directory: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (index, mode) in ["r", "rb", "r+e", "w", "w+t", "a", "a+b", "x", "x+e", "c+"]
        .into_iter()
        .enumerate()
    {
        for (scenario, tail) in [
            "var_dump(ftell($f), feof($f), fread($f, 2), ftell($f), feof($f));",
            "var_dump(fwrite($f, 'XY'), ftell($f));",
            "var_dump(fseek($f, 2), ftell($f), fread($f, 2), fwrite($f, 'Z'), ftell($f));",
            "var_dump(fseek($f, 0, SEEK_END), ftell($f), fread($f, 1), feof($f));",
            "var_dump(rewind($f), fgets($f), fgets($f), ftell($f), feof($f));",
        ]
        .into_iter()
        .enumerate()
        {
            let initial = if mode.starts_with('x') {
                ""
            } else {
                r#"file_put_contents('fixture.bin', "ab\r\nxy\ntail");"#
            };
            paths.push(save(
                directory,
                &format!("mode_{index}_{scenario}"),
                &format!(
                    "<?php {initial}\n$f = fopen('fixture.bin', '{mode}');\n{tail}\n\
                 fclose($f); var_dump(bin2hex(file_get_contents('fixture.bin')));"
                ),
            ));
        }
    }
    for (index, field) in [
        r#""abc""#,
        r#"" a, b ""#,
        r#""a;b""#,
        r#""has\"quote""#,
        r#""a\\b""#,
        r#""line\r\nend""#,
        r#""n\0ul""#,
        r#""""#,
        r#""\xFF\xFE""#,
        r#""\t""#,
    ]
    .into_iter()
    .enumerate()
    {
        for (dialect, (separator, enclosure, escape)) in [
            (r#"",""#, r#""\"""#, r#""""#),
            (r#"";""#, r#""'""#, r#""\\""#),
            (r#""\t""#, r#""*""#, r#""""#),
        ]
        .into_iter()
        .enumerate()
        {
            paths.push(save(
                directory,
                &format!("csv_{index}_{dialect}"),
                &format!(
                    "<?php\n$f = fopen('php://memory', 'w+');\n\
                 var_dump(fputcsv($f, [{field}, '', 'end'], {separator}, {enclosure}, {escape}));\n\
                 rewind($f); var_dump(bin2hex(stream_get_contents($f)));\n\
                 rewind($f); var_dump(fgetcsv($f, 0, {separator}, {enclosure}, {escape}));"
                ),
            ));
        }
    }
    paths
}

fn save(directory: &Path, name: &str, source: &str) -> PathBuf {
    let path = directory.join(format!("{name}.php"));
    fs::write(&path, source).expect("write generated comparison script");
    path
}
