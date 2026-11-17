//! Replay both PHP libraries' recorded results. Each divergence is selected by syntax rule.

use std::collections::BTreeMap;
use std::path::Path;

use phera_dotenv::{Environment, parse};
use serde_json::Value;

type Pairs = Option<BTreeMap<Vec<u8>, Vec<u8>>>;

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn recorded(row: &Value) -> Pairs {
    row.get("pairs")?.as_array().map(|pairs| {
        pairs
            .iter()
            .filter_map(|pair| Some((bytes(pair[0].as_str().unwrap()), bytes(pair[1].as_str()?))))
            .collect()
    })
}

fn patch(mut base: Pairs, key: &[u8], value: &[u8]) -> Pairs {
    base.as_mut().unwrap().insert(key.to_vec(), value.to_vec());
    base
}

fn expected(id: &str, input: &[u8], vlucas: &Pairs, symfony: &Pairs) -> Pairs {
    if let Some(case) = id.strip_prefix("matrix-") {
        let kind = case.split('/').next().unwrap();
        let line = input.split(|b| *b == b'\n').nth(1).unwrap();
        let rhs = line
            .splitn(2, |b| *b == b'=')
            .nth(1)
            .unwrap()
            .trim_ascii_start();
        let single = rhs.starts_with(b"'");
        // Combine the independent grammar choices even when neither library
        // accepts this exact combination (e.g. padded '=' plus bare expansion).
        let value: &[u8] = match kind {
            "plain" => b"string",
            "hash" => b"one#two",
            "empty" => b"",
            "space" => b"two words",
            "brace" if single => b"${BASE}",
            "dollar" if single => b"$BASE",
            "brace" | "dollar" => b"example",
            "default" if single => b"${ABSENT:-yes}",
            "default" => b"yes",
            "escape" if rhs.starts_with(b"\"") => b"a\nb",
            "escape" => br"a\nb",
            "unknown" => br"a\qb",
            _ => panic!("unknown class {kind}"),
        };
        return Some(BTreeMap::from([
            (b"BASE".to_vec(), b"example".to_vec()),
            (b"KEY".to_vec(), value.to_vec()),
        ]));
    }
    if id.ends_with("/commented.env") {
        return patch(vlucas.clone(), b"FOOO", b"foo# comment with no space");
    }
    if id.ends_with("/nested.env") {
        return patch(
            patch(vlucas.clone(), b"NVAR3", "{Hellō} {World!}".as_bytes()),
            b"NVAR5",
            "Hellō {NVAR2}".as_bytes(),
        );
    }
    if id == "bytes/1" || id.ends_with("/nullbyte.env") {
        return None;
    }
    if let Some(case) = id.strip_prefix("unquoted-spaces/") {
        let value: &[u8] = if case == "0" {
            b"hello world"
        } else {
            b"one\ttwo"
        };
        return Some(BTreeMap::from([(b"A".to_vec(), value.to_vec())]));
    }
    if (id.starts_with("expansion/") && id != "expansion/3")
        || matches!(id, "comment/2" | "bytes/0" | "quoted/0" | "escapes/4")
        || id.starts_with("command/")
        || id.ends_with("/windows.env")
        || id.ends_with("/specialchars.env")
    {
        symfony.clone()
    } else {
        vlucas.clone()
    }
}

#[test]
fn recorded_php_libraries() {
    let mut external = Environment::default();
    external.insert(b"EXTERNAL".to_vec(), b"outside".to_vec());
    let lines = include_str!("oracles.jsonl").lines().skip(1);
    let mut counts = BTreeMap::<String, [usize; 4]>::new();
    let mut mismatches = Vec::new();
    for line in lines {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        let input = bytes(row["input"].as_str().unwrap());
        let vlucas = recorded(&row["vlucas"]);
        let symfony = recorded(&row["symfony"]);
        let wanted = expected(id, &input, &vlucas, &symfony);
        let actual = parse(
            Path::new(".env"),
            &input,
            &external,
            &Environment::default(),
        )
        .ok()
        .map(|env| env.iter().map(|(k, v)| (k.to_vec(), v.to_vec())).collect());
        let class = id.split('/').next().unwrap();
        let index = match (actual == vlucas, actual == symfony) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        counts.entry(class.to_owned()).or_default()[index] += 1;
        if actual != wanted {
            mismatches.push(id.to_owned());
        }
    }
    for (class, counts) in counts {
        println!("{class} {counts:?}");
    }
    assert!(mismatches.is_empty(), "{mismatches:?}");
}
