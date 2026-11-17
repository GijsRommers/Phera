//! Byte-level and token-level mutation of PHP source seeds.

use crate::rng::Rng;

const INTERESTING_BYTES: &[u8] = b"()[]{};,.+-*/%<>=!&|?:$@'\"`\\ \n\t\r#~^0123456789abcxyzABCXYZ_";

const KEYWORDS: &[&str] = &[
    "array", "fn", "function", "return", "if", "else", "elseif", "while", "for", "foreach",
    "as", "try", "catch", "finally", "throw", "new", "clone", "class", "extends", "implements",
    "interface", "trait", "enum", "match", "case", "default", "break", "continue", "echo",
    "print", "global", "static", "public", "private", "protected", "readonly", "abstract",
    "final", "yield", "from", "and", "or", "xor", "instanceof", "insteadof", "use", "namespace",
    "declare", "strict_types", "list", "isset", "unset", "empty", "eval", "include",
    "include_once", "require", "require_once", "goto", "switch", "do", "true", "false",
    "null", "int", "float", "string", "bool", "array", "object", "callable", "void", "never",
    "self", "parent", "static", "??", "?->", "...", "=>", "**", "<<", ">>", "<=>", "===", "!==",
    r"&$x", r"&$a", r"&$this", "[$a]", "compact", "extract", "get_defined_vars", "call_user_func",
    "array_map", "usort", "uasort", "uksort", "array_filter", "array_reduce", "sprintf",
    "preg_match", "preg_replace", "json_decode", "json_encode", "serialize", "unserialize",
    "var_dump", "print_r", "str_repeat", "range", "array_fill", "str_pad", "implode",
    "explode", "number_format", "unpack", "pack", "chr", "ord", "hex2bin", "base64_decode",
];

const BOUNDARY_NUMBERS: &[&str] = &[
    "0", "-1", "1", "2", "255", "256", "65535", "2147483647", "2147483648", "4294967295",
    "4294967296", "9223372036854775807", "-9223372036854775808", "9223372036854775808",
    "-9223372036854775809", "18446744073709551615", "1e308", "-1e308", "1e309", "-1e309",
    "NAN", "INF", "-INF", "0.1", "1e-300", "0.5", "0x7fffffffffffffff", "0x8000000000000000",
    "0xffffffffffffffff", "0b1111111111111111111111111111111", "017777777777",
    "1000000", "999999", "65536", "100000",
];

/// A rough token: identifier/number, quoted string, whitespace run, or one
/// punctuation byte. Good enough to splice at token boundaries.
#[derive(Clone, Debug)]
enum Tok {
    Word(usize, usize),
    Str(usize, usize),
    Space(usize, usize),
    Punct(#[allow(dead_code)] u8, usize),
}

fn tokenize(bytes: &[u8]) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            let start = i;
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            toks.push(Tok::Space(start, i));
        } else if b == b'"' || b == b'\'' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            toks.push(Tok::Str(start, i));
        } else if b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80 {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] >= 0x80)
            {
                i += 1;
            }
            toks.push(Tok::Word(start, i));
        } else {
            toks.push(Tok::Punct(b, i));
            i += 1;
        }
    }
    toks
}

fn is_word(_bytes: &[u8], tok: &Tok) -> bool {
    matches!(tok, Tok::Word(..))
}

/// Applies `count` random mutations to `src`, returning the new source and a
/// human-readable description of what was done.
pub fn mutate(src: &[u8], other: &[u8], rng: &mut Rng, count: usize, max_len: usize) -> (Vec<u8>, String) {
    let mut buf = src.to_vec();
    let mut log = String::new();
    for _ in 0..count {
        if buf.is_empty() {
            buf.extend_from_slice(b"<?php ");
        }
        let op = rng.below(11);
        let before = buf.len();
        match op {
            0 => {
                // flip a byte
                let p = rng.below(buf.len());
                buf[p] = INTERESTING_BYTES[rng.below(INTERESTING_BYTES.len())];
                log.push_str(&format!("flip@{p} "));
            }
            1 => {
                // insert a byte
                let p = rng.below(buf.len() + 1);
                buf.insert(p, INTERESTING_BYTES[rng.below(INTERESTING_BYTES.len())]);
                log.push_str(&format!("ins@{p} "));
            }
            2 => {
                // delete a byte
                let p = rng.below(buf.len());
                buf.remove(p);
                log.push_str(&format!("del@{p} "));
            }
            3 => {
                // duplicate a slice
                let a = rng.below(buf.len());
                let b = a + rng.below((buf.len() - a).min(4096) + 1);
                let slice = buf[a..b].to_vec();
                buf.splice(a..a, slice);
                log.push_str(&format!("dup[{a}..{b}] "));
            }
            4 => {
                // delete a slice
                let a = rng.below(buf.len());
                let b = a + rng.below((buf.len() - a).min(4096) + 1);
                buf.drain(a..b);
                log.push_str(&format!("drop[{a}..{b}] "));
            }
            5 => {
                // splice in a slice from another seed
                if !other.is_empty() && buf.len() > 4 {
                    let a = rng.below(other.len());
                    let b = a + rng.below((other.len() - a).min(4096) + 1);
                    let p = rng.below(buf.len());
                    buf.splice(p..p, other[a..b].to_vec());
                    log.push_str(&format!("splice[{a}..{b}]@{p} "));
                }
            }
            6 | 7 => {
                // token-level ops
                let toks = tokenize(&buf);
                if !toks.is_empty() {
                    let t = rng.below(toks.len());
                    match rng.below(4) {
                        0 => {
                            // duplicate a token
                            let (a, b) = span(&buf, &toks[t]);
                            let slice = buf[a..b].to_vec();
                            buf.splice(b..b, slice);
                            log.push_str(&format!("tokdup[{a}..{b}] "));
                        }
                        1 => {
                            // delete a token
                            let (a, b) = span(&buf, &toks[t]);
                            buf.drain(a..b);
                            log.push_str(&format!("tokdel[{a}..{b}] "));
                        }
                        2 => {
                            // insert a keyword
                            let kw = rng.pick(KEYWORDS).as_bytes().to_vec();
                            let kwlen = kw.len();
                            let (a, _) = span(&buf, &toks[t]);
                            buf.splice(a..a, kw);
                            buf.splice(a + kwlen..a + kwlen, vec![b' ']);
                            log.push_str("tokkw ");
                        }
                        _ => {
                            // swap two tokens
                            let u = rng.below(toks.len());
                            let (a1, b1) = span(&buf, &toks[t]);
                            let (a2, b2) = span(&buf, &toks[u]);
                            if a1 != a2 {
                                let s1 = buf[a1..b1].to_vec();
                                let s2 = buf[a2..b2].to_vec();
                                if s1.len() == s2.len() {
                                    buf.splice(a1..b1, s2);
                                    buf.splice(a2..b2, s1);
                                    log.push_str("tokswap ");
                                }
                            }
                        }
                    }
                }
            }
            8 => {
                // replace a numeric literal with a boundary value
                let toks = tokenize(&buf);
                let words: Vec<&Tok> = toks.iter().filter(|t| is_word(&buf, t)).collect();
                let numeric: Vec<&&Tok> = words
                    .iter()
                    .filter(|t| {
                        if let Tok::Word(a, b) = t {
                            buf[*a..*b].iter().any(|c| c.is_ascii_digit())
                        } else {
                            false
                        }
                    })
                    .collect();
                if !numeric.is_empty() {
                    let t = *rng.pick(&numeric);
                    if let Tok::Word(a, b) = t {
                        let repl = rng.pick(BOUNDARY_NUMBERS).as_bytes().to_vec();
                        buf.splice(*a..*b, repl);
                        log.push_str("numrepl ");
                    }
                }
            }
            9 => {
                // replace an identifier with a keyword or another word from the file
                let toks = tokenize(&buf);
                let words: Vec<&Tok> = toks.iter().filter(|t| is_word(&buf, t)).collect();
                if !words.is_empty() {
                    let t = *rng.pick(&words);
                    if let Tok::Word(a, b) = t {
                        let repl: Vec<u8> = if rng.chance(1, 2) {
                            rng.pick(KEYWORDS).as_bytes().to_vec()
                        } else {
                            let u = *rng.pick(&words);
                            if let Tok::Word(a2, b2) = u {
                                buf[*a2..*b2].to_vec()
                            } else {
                                b"x".to_vec()
                            }
                        };
                        buf.splice(*a..*b, repl);
                        log.push_str("idrepl ");
                    }
                }
            }
            _ => {
                // wrap a random token-aligned slice in a nesting construct
                let toks = tokenize(&buf);
                if toks.len() > 2 {
                    let a = rng.below(toks.len() - 1);
                    let b = a + 1 + rng.below((toks.len() - a - 1).min(64));
                    let (sa, _) = span(&buf, &toks[a]);
                    let (_, sb) = span(&buf, &toks[b - 1]);
                    let wraps: &[&str] = &[
                        "if (true) { ", " }\n",
                        "try { ", " } catch (Throwable $e) { }\n",
                        "array( ", " )\n",
                        "( ", " )\n",
                        "function () { ", " };\n",
                        "[$x = ", "]\n",
                    ];
                    let w = rng.below(wraps.len() / 2);
                    let mut piece: Vec<u8> = wraps[2 * w].as_bytes().to_vec();
                    piece.extend_from_slice(&buf[sa..sb]);
                    piece.extend_from_slice(wraps[2 * w + 1].as_bytes());
                    buf.splice(sa..sb, piece);
                    log.push_str(&format!("wrap{w}[{sa}..{sb}] "));
                }
            }
        }
        if buf.len() > max_len {
            buf.truncate(max_len);
            log.push_str("trunc ");
            break;
        }
        if buf.len() != before && log.len() > 400 {
            break;
        }
    }
    if buf.windows(5).any(|w| w == b"<?php") || buf.windows(4).any(|w| w == b"<?=") {
        // still a PHP file, fine
    } else if !buf.is_empty() {
        let mut with_open = b"<?php\n".to_vec();
        with_open.extend_from_slice(&buf);
        buf = with_open;
        log.push_str("addopen ");
    }
    (buf, log)
}

fn span(_bytes: &[u8], tok: &Tok) -> (usize, usize) {
    match tok {
        Tok::Word(a, b) | Tok::Str(a, b) | Tok::Space(a, b) => (*a, *b),
        Tok::Punct(_, p) => (*p, *p + 1),
    }
}
