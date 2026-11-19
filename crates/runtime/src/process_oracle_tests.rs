//! Compare packed addresses, scan results, escaping and entity encodings
//! with PHP, including `sscanf('42', '%d', $number)` by-reference writes.

mod execution;
mod posix;

use std::{fmt::Write, sync::Arc};

use phera_extension_api::testing::oracle;

struct Cases {
    script: String,
    labels: Vec<String>,
}

impl Cases {
    fn new() -> Self {
        let mut script = String::from("<?php\n");
        script.push_str(oracle::PHP_PREAMBLE);
        Self {
            script,
            labels: Vec::new(),
        }
    }

    fn check(&mut self, body: String) {
        writeln!(
            self.script,
            "run('{}', function () {{ {body} }});",
            self.labels.len()
        )
        .unwrap();
        self.labels.push(body);
    }

    fn result(&mut self, expression: String) {
        self.check(format!("return {expression};"));
    }

    async fn compare(self, name: &str) {
        let Some(expected) = oracle::run_php(name, &self.script) else {
            return;
        };
        let runtime = super::Runtime::new();
        let unit = phera_compiler::compile(&self.script, "process-oracle.php").unwrap();
        let mut vm = runtime.new_cli_vm();
        assert_eq!(vm.run(Arc::new(unit)).await, 0);
        let actual = String::from_utf8_lossy(&vm.output)
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        oracle::compare(name, &self.labels, &expected, &actual);
    }
}

fn input(bytes: &[u8]) -> String {
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("hex2bin('{hex}')")
}

#[tokio::test(flavor = "current_thread")]
async fn packed_ips_and_command_escaping_match_php() {
    let mut cases = Cases::new();
    for bytes in [
        b"".as_slice(),
        b"1.2.3.4",
        b"1.2.3.04",
        b"127.1",
        b"0256.01.02.03",
        b"255.255.255.255",
        b"256.255.255.255",
        b" 1.2.3.4",
        b"1.2.3.4\n",
        b"127.0.0.1\0",
        b"::",
        b"::1",
        b"::0.0.1.2",
        b"::1.2.3.4",
        b"::ffff:1.2.3.4",
        b"::ffff:1.2.3.004",
        b"2001:DB8:0:1:0:0:0:1",
        b"::FFFF:192.0.2.1",
        b"12:34::56::78",
        b"fe80::a%1",
        b"fe80::b%phera_nonexistent",
        b"\xff",
    ] {
        let input = input(bytes);
        for function in ["ip2long", "inet_pton"] {
            cases.result(format!("{function}({input})"));
        }
        cases.check(format!(
            "$bytes = inet_pton({input}); return $bytes === false ? false : inet_ntop($bytes);"
        ));
    }
    for bytes in [
        vec![255; 4],
        vec![255; 16],
        vec![],
        vec![0; 16],
        vec![0; 3],
        vec![0; 17],
    ] {
        cases.result(format!("inet_ntop({})", input(&bytes)));
    }
    for number in [
        i64::MIN,
        -4_294_967_297,
        -1,
        0,
        1,
        2_147_483_648,
        4_294_967_295,
        4_294_967_296,
        i64::MAX,
    ] {
        cases.result(format!("long2ip({number})"));
    }
    let quotes = [b"".as_slice(), b"'", b"\"", b"'a'", b"\"a\"", b"'\""];
    for left in quotes {
        for right in quotes {
            for middle in [
                b"".as_slice(),
                b"a;|&\n",
                b"#?[]{}()%^!~=$\\",
                b"\xff",
                b"\0",
            ] {
                let bytes = [left, middle, right].concat();
                cases.result(format!("escapeshellcmd({})", input(&bytes)));
            }
        }
    }
    cases.compare("process-addresses-and-shell").await;
}

#[tokio::test(flavor = "current_thread")]
async fn html_and_scanning_match_php() {
    let mut cases = Cases::new();
    let bytes = [
        b"".as_slice(),
        "Æ © & fjord ≂̸".as_bytes(),
        b"\xe9\x80\x85\xff &excl; &#xA0;",
        b"'\">@!\n\x01",
        b"\xc0\xaf\xef\xbf\xbf\0",
        b"\xe2\x82",
        "≩︀ ∾̳ <⃒".as_bytes(),
    ];
    for bytes in bytes {
        let input = input(bytes);
        for flags in [0, 2, 3, 4, 8, 16, 32, 48, 51, 179, 208] {
            for encoding in ["UTF-8", "ISO-8859-1", "CP1252", "EUC-JP", "unsupported"] {
                cases.result(format!("htmlentities({input},{flags},'{encoding}', false)"));
            }
        }
    }

    let formats = [
        "%d",
        "%2$d|%1$s",
        "%d%n",
        "%3c%n",
        "%*d %x",
        "%i %o",
        "%u",
        "%E%n",
        "%[d-a]",
        "%[^,]",
        "%",
        "%[",
        "%%%s",
        "%b",
        "%s%c",
    ];
    for bytes in [
        "",
        "9",
        "0x9 |x",
        "014 777",
        "-9223372036854775809",
        "a\0tail",
        "abcd",
        "+1.25e-4",
        "9exa",
        "%y",
        "1.3e+q",
        " 42",
    ] {
        let bytes = input(bytes.as_bytes());
        for format in formats {
            cases.result(format!("sscanf({bytes}, '{format}')"));
            cases.check(format!(
                "$first = 'unchanged'; $second = null; return [sscanf({bytes}, '{format}', $first, $second), $first, $second];"
            ));
        }
    }
    cases.compare("process-html-and-scanning").await;
}
