//! PHP image metadata probes the XBM definitions and explicit SVG dimensions.

use super::Size;

pub(crate) fn xbm(data: &[u8]) -> Option<Size> {
    let mut width = None;
    let mut height = None;
    let text = std::str::from_utf8(data.get(..data.len().min(32_768))?).ok()?;
    for line in text.lines() {
        let mut fields = line.split_ascii_whitespace();
        if fields.next() != Some("#define") {
            continue;
        }
        let Some(key) = fields.next() else { continue };
        let dimension = if key.ends_with("_width") {
            &mut width
        } else if key.ends_with("_height") {
            &mut height
        } else {
            continue;
        };
        *dimension = fields.next().and_then(|digits| digits.parse().ok());
        if let (Some(w), Some(h)) = (width, height) {
            return Some(Size::new(w, h, 0, 0));
        }
    }
    None
}

pub(crate) fn svg(data: &[u8]) -> Option<Size> {
    let limit = data.len().min(32_768);
    let text = std::str::from_utf8(data.get(..limit)?).ok()?;
    let mut text = text.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(rem) = text.strip_prefix("<?") {
            text = rem.split_once("?>")?.1.trim_start();
        } else if let Some(rem) = text.strip_prefix("<!--") {
            text = rem.split_once("-->")?.1.trim_start();
        } else {
            break;
        }
    }
    let element = text.strip_prefix("<svg")?;
    if !element.starts_with(char::is_whitespace) {
        return None;
    }
    let (mut width, mut height) = (None, None);
    let mut rest = element;
    while !rest.trim_start().starts_with(['>', '/']) {
        rest = rest.trim_start();
        let end = rest.find(|ch: char| ch.is_whitespace() || ch == '=')?;
        let (name, tail) = rest.split_at(end);
        let tail = tail.trim_start().strip_prefix('=')?.trim_start();
        let quote = tail.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let (value, tail) = tail.get(1..)?.split_once(quote)?;
        match name {
            "width" => width = dimension(value),
            "height" => height = dimension(value),
            _ => {}
        }
        rest = tail;
    }
    let (width, width_unit) = width?;
    let (height, height_unit) = height?;
    Some(Size {
        width,
        height,
        width_unit,
        height_unit,
        bits: 0,
        channels: 0,
    })
}

fn dimension(text: &str) -> Option<(u32, &'static str)> {
    let number = text.bytes().take_while(u8::is_ascii_digit).count();
    let unit = match text.get(number..)? {
        "" | "px" => "px",
        "cm" => "cm",
        "mm" => "mm",
        "in" => "in",
        "pt" => "pt",
        "pc" => "pc",
        _ => return None,
    };
    Some((text.get(..number)?.parse().ok()?, unit))
}
