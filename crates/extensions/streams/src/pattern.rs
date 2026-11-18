//! `fnmatch('*.php', 'index.php')` matches a shell pattern against path bytes.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::errors::{check_path, warn_false};

const NOESCAPE: i64 = nix::libc::FNM_NOESCAPE as i64;
const PATHNAME: i64 = nix::libc::FNM_PATHNAME as i64;
const PERIOD: i64 = nix::libc::FNM_PERIOD as i64;
const CASEFOLD: i64 = 16;

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("pattern", ParamType::String),
        Param::new("filename", ParamType::String),
        Param::new("flags", ParamType::Int).optional(),
    ];
    registry.function(FunctionEntry::sync("fnmatch", PARAMS, fnmatch));
    for (name, value) in [
        ("FNM_NOESCAPE", NOESCAPE),
        ("FNM_PATHNAME", PATHNAME),
        ("FNM_PERIOD", PERIOD),
        ("FNM_CASEFOLD", CASEFOLD),
    ] {
        registry.constant(name, value);
    }
}

fn fnmatch(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let pattern = args::bytes(arguments, 0);
    let name = args::bytes(arguments, 1);
    check_path("fnmatch", "#1 ($pattern)", pattern)?;
    check_path("fnmatch", "#2 ($filename)", name)?;
    let limit = if cfg!(target_os = "linux") {
        4096
    } else {
        1024
    };
    for (argument, bytes) in [("Pattern", pattern), ("Filename", name)] {
        if bytes.len() >= limit {
            return warn_false(
                context,
                format!(
                    "fnmatch(): {argument} exceeds the maximum allowed length of {limit} characters"
                ),
            );
        }
    }
    let flags = args::int(arguments, 2).unwrap_or(0);
    let options = Options {
        escape: flags & NOESCAPE == 0,
        path: flags & PATHNAME != 0,
        period: flags & PERIOD != 0,
        fold: flags & CASEFOLD != 0,
    };
    Ok(Value::Bool(options.matches(pattern, name)))
}

pub struct Options {
    pub escape: bool,
    pub path: bool,
    pub period: bool,
    pub fold: bool,
}

enum Token<'a> {
    Star,
    Any,
    Literal(u8),
    Class(&'a [u8]),
    Never,
}

impl Options {
    pub fn matches(&self, pattern: &[u8], name: &[u8]) -> bool {
        let mut pattern = pattern;
        let mut offset = 0;
        let mut star = None;
        while !pattern.is_empty() || offset < name.len() {
            let (token, rest) = self.token(pattern);
            let input = name.get(offset).copied();
            let wild = self.wild(name, offset);
            match (token, input) {
                (Token::Star, _) if !self.hidden(name, offset) => {
                    star = Some((rest, offset));
                    pattern = rest;
                    continue;
                }
                (Token::Literal(literal), Some(byte)) if self.equal(literal, byte) => (),
                (Token::Any, Some(_)) if wild => (),
                (Token::Class(class), Some(byte)) if wild && self.class(class, byte) => (),
                _ => {
                    if let Some((restart, spent)) = star
                        && self.wild(name, spent)
                    {
                        offset = spent + 1;
                        star = Some((restart, offset));
                        pattern = restart;
                        continue;
                    }
                    return false;
                }
            }
            pattern = rest;
            offset += 1;
        }
        true
    }

    fn token<'a>(&self, pattern: &'a [u8]) -> (Token<'a>, &'a [u8]) {
        match pattern {
            [b'*', rest @ ..] => (Token::Star, rest),
            [b'?', rest @ ..] => (Token::Any, rest),
            [b'\\', byte, rest @ ..] if self.escape => (Token::Literal(*byte), rest),
            [b'\\'] if self.escape => (Token::Never, &[]),
            [b'[', rest @ ..] => match class_end(rest) {
                Some(index) => (Token::Class(&rest[..index]), &rest[index + 1..]),
                None => (Token::Literal(b'['), rest),
            },
            [byte, rest @ ..] => (Token::Literal(*byte), rest),
            [] => (Token::Never, &[]),
        }
    }

    fn hidden(&self, name: &[u8], offset: usize) -> bool {
        self.period
            && name.get(offset) == Some(&b'.')
            && (offset == 0 || (self.path && name.get(offset - 1) == Some(&b'/')))
    }

    fn wild(&self, name: &[u8], offset: usize) -> bool {
        name.get(offset)
            .is_some_and(|byte| !self.path || *byte != b'/')
            && !self.hidden(name, offset)
    }

    fn equal(&self, left: u8, right: u8) -> bool {
        left == right || (self.fold && left.eq_ignore_ascii_case(&right))
    }

    fn class(&self, class: &[u8], byte: u8) -> bool {
        let (negated, mut class) = match class {
            [b'!' | b'^', rest @ ..] => (true, rest),
            _ => (false, class),
        };
        let mut matched = false;
        while !class.is_empty() {
            if let Some((name, rest)) = named(class) {
                matched |= character_class(name, byte);
                class = rest;
                continue;
            }
            let Some((first, rest)) = class_byte(class, self.escape) else {
                break;
            };
            if let Some(range) = rest.strip_prefix(b"-")
                && let Some((last, rest)) = class_byte(range, self.escape)
            {
                let normalize = |b: u8| if self.fold { b.to_ascii_lowercase() } else { b };
                matched |= (normalize(first)..=normalize(last)).contains(&normalize(byte));
                class = rest;
            } else {
                matched |= self.equal(first, byte);
                class = rest;
            }
        }
        matched != negated
    }

    pub fn literal(&self, pattern: &[u8]) -> Option<Vec<u8>> {
        let mut remaining = pattern;
        let mut output = Vec::new();
        while !remaining.is_empty() {
            let (token, rest) = self.token(remaining);
            let Token::Literal(byte) = token else {
                return None;
            };
            output.push(byte);
            remaining = rest;
        }
        Some(output)
    }
}

fn class_byte(class: &[u8], escape: bool) -> Option<(u8, &[u8])> {
    match class {
        [b'\\', byte, rest @ ..] if escape => Some((*byte, rest)),
        [byte, rest @ ..] => Some((*byte, rest)),
        [] => None,
    }
}

fn named(class: &[u8]) -> Option<(&[u8], &[u8])> {
    let name = class.strip_prefix(b"[:")?;
    let end = name.windows(2).position(|part| part == b":]")?;
    Some((&name[..end], &name[end + 2..]))
}

fn class_end(class: &[u8]) -> Option<usize> {
    let mut index = usize::from(matches!(class.first(), Some(b'!' | b'^')));
    if class.get(index) == Some(&b']') {
        index += 1;
    }
    while let Some(byte) = class.get(index) {
        if let Some((_, rest)) = named(&class[index..]) {
            index = class.len() - rest.len();
            continue;
        }
        if *byte == b']' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn character_class(name: &[u8], byte: u8) -> bool {
    match name {
        b"alnum" => byte.is_ascii_alphanumeric(),
        b"alpha" => byte.is_ascii_alphabetic(),
        b"blank" => matches!(byte, b' ' | b'\t'),
        b"cntrl" => byte.is_ascii_control(),
        b"digit" => byte.is_ascii_digit(),
        b"graph" => byte.is_ascii_graphic(),
        b"lower" => byte.is_ascii_lowercase(),
        b"print" => byte.is_ascii_graphic() || byte == b' ',
        b"punct" => byte.is_ascii_punctuation(),
        b"space" => byte.is_ascii_whitespace() || byte == 11,
        b"upper" => byte.is_ascii_uppercase(),
        b"xdigit" => byte.is_ascii_hexdigit(),
        _ => false,
    }
}
