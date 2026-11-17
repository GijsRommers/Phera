//! Encoding names are PHP names: `mb_encoding_aliases('UTF-8') === ['utf8']`.
use crate::state::Settings;
use phera_extension_api::{Context, NativeError, Value, args};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoding(pub usize);

impl Encoding {
    pub const UTF8: Self = Self(14);
    pub fn name(self) -> &'static str {
        self.metadata().0
    }
    pub fn metadata(self) -> (&'static str, &'static str, &'static str) {
        if self.0 == usize::MAX {
            return ("pass", "", "");
        }
        let mut fields = include_str!("../data/encodings.txt")
            .lines()
            .nth(self.0)
            .unwrap_or_default()
            .split('|');
        (
            fields.next().unwrap_or_default(),
            fields.next().unwrap_or_default(),
            fields.next().unwrap_or_default(),
        )
    }
    pub fn find(name: &str) -> Option<Self> {
        include_str!("../data/encodings.txt")
            .lines()
            .enumerate()
            .find_map(|(i, line)| {
                let mut fields = line.split('|');
                let canonical = fields.next()?;
                let aliases = fields.next()?;
                let mime = fields.next()?;
                (canonical.eq_ignore_ascii_case(name)
                    || (!mime.is_empty() && mime.eq_ignore_ascii_case(name))
                    || aliases
                        .split(',')
                        .any(|alias| !alias.is_empty() && alias.eq_ignore_ascii_case(name)))
                .then_some(Self(i))
            })
    }
    pub fn all() -> impl Iterator<Item = Self> {
        (0..include_str!("../data/encodings.txt").lines().count()).map(Self)
    }
    pub fn single(self) -> Option<&'static [u8]> {
        match self.0 {
            22 => Some(include_bytes!("../data/single-22.bin")),
            23 => Some(include_bytes!("../data/single-23.bin")),
            24 => Some(include_bytes!("../data/single-24.bin")),
            25 => Some(include_bytes!("../data/single-25.bin")),
            26 => Some(include_bytes!("../data/single-26.bin")),
            27 => Some(include_bytes!("../data/single-27.bin")),
            28 => Some(include_bytes!("../data/single-28.bin")),
            29 => Some(include_bytes!("../data/single-29.bin")),
            30 => Some(include_bytes!("../data/single-30.bin")),
            31 => Some(include_bytes!("../data/single-31.bin")),
            32 => Some(include_bytes!("../data/single-32.bin")),
            33 => Some(include_bytes!("../data/single-33.bin")),
            34 => Some(include_bytes!("../data/single-34.bin")),
            35 => Some(include_bytes!("../data/single-35.bin")),
            36 => Some(include_bytes!("../data/single-36.bin")),
            37 => Some(include_bytes!("../data/single-37.bin")),
            41 => Some(include_bytes!("../data/single-41.bin")),
            42 => Some(include_bytes!("../data/single-42.bin")),
            43 => Some(include_bytes!("../data/single-43.bin")),
            _ => None,
        }
    }
    pub fn web(self) -> Option<&'static encoding_rs::Encoding> {
        match self.0 {
            16 => Some(encoding_rs::EUC_JP),
            17..=19 => Some(encoding_rs::SHIFT_JIS),
            20 => Some(encoding_rs::ISO_2022_JP),
            21 => Some(encoding_rs::GB18030),
            38 => Some(encoding_rs::GBK),
            39 => Some(encoding_rs::BIG5),
            40 => Some(encoding_rs::EUC_KR),
            _ => None,
        }
    }
}

pub fn argument(
    ctx: &mut dyn Context,
    a: &[Value],
    i: usize,
    function: &str,
) -> Result<Encoding, NativeError> {
    match args::string(a, i) {
        Some(name) => {
            let name = name.to_string_lossy();
            let enc = resolve(&name, function, i + 1, "encoding")?;
            crate::pseudo::selected(ctx, &name, enc, function);
            Ok(enc)
        }
        None => Ok(ctx.state().get_or_default::<Settings>().internal),
    }
}
pub fn resolve(
    name: &str,
    function: &str,
    number: usize,
    parameter: &str,
) -> Result<Encoding, NativeError> {
    Encoding::find(name).ok_or_else(|| NativeError::value_error(format!(
        "{function}(): Argument #{number} (${parameter}) must be a valid encoding, \"{name}\" given"
    )))
}
pub fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
