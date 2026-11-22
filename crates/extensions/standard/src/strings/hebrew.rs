//! `hebrev($text, $max_chars_per_line)` converts ISO-8859-8 logical order to visual order.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, Value, args,
};

pub(super) const ENTRY: FunctionEntry = FunctionEntry::sync(
    "hebrev",
    &[
        Param::new("string", ParamType::String),
        Param::new("max_chars_per_line", ParamType::Int).with_default(ConstantValue::Int(0)),
    ],
    hebrev,
);

fn hebrev(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let width = args::int(arguments, 1).unwrap_or_default();
    let mut output = Vec::with_capacity(input.len());
    for line in input.split_inclusive(|&byte| matches!(byte, b'\r' | b'\n')) {
        let (content, ending) = match line.split_last() {
            Some((&ending, content)) if matches!(ending, b'\r' | b'\n') => (content, Some(ending)),
            _ => (line, None),
        };
        let mut visual = visual_order(content);
        if width < 0 {
            visual.reverse();
            output.extend_from_slice(&visual);
        } else {
            write_wrapped(&mut output, &visual, width as usize);
        }
        output.extend(ending);
    }
    Ok(Value::from(output))
}

fn visual_order(input: &[u8]) -> Vec<u8> {
    if !input.iter().any(is_hebrew) {
        return input.to_vec();
    }
    let mut visual: Vec<_> = input.iter().rev().copied().map(mirror).collect();
    // The outside punctuation follows the right-to-left text. In the middle
    // of a Latin fragment, spaces and punctuation keep their original order.
    for fragment in visual.split_mut(is_hebrew) {
        if let (Some(start), Some(end)) = (
            fragment.iter().position(u8::is_ascii_alphanumeric),
            fragment.iter().rposition(u8::is_ascii_alphanumeric),
        ) {
            fragment[start..=end].reverse();
        }
    }
    visual
}

fn is_hebrew(byte: &u8) -> bool {
    (0xe0..=0xfa).contains(byte)
}

fn mirror(byte: u8) -> u8 {
    match byte {
        b'(' => b')',
        b')' => b'(',
        b'[' => b']',
        b']' => b'[',
        b'{' => b'}',
        b'}' => b'{',
        b'<' => b'>',
        b'>' => b'<',
        _ => byte,
    }
}

fn write_wrapped(output: &mut Vec<u8>, mut visual: &[u8], width: usize) {
    while width != 0 && visual.len() > width {
        let start = visual.len() - width - 1;
        let Some(separator) = visual[start..].iter().position(u8::is_ascii_whitespace) else {
            break;
        };
        let separator = start + separator;
        if separator == 0 || separator + 1 == visual.len() {
            break;
        }
        output.extend_from_slice(&visual[separator + 1..]);
        output.push(b'\n');
        visual = &visual[..separator];
    }
    output.extend_from_slice(visual);
}
