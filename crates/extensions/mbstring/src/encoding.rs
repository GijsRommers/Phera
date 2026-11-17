//! The encodings mbstring knows, under PHP's names and aliases.
//!
//! ```php
//! mb_encoding_aliases('UTF-8');      // ['utf8']
//! mb_preferred_mime_name('SJIS');    // 'Shift_JIS'
//! mb_strlen("\xE9", 'latin1');       // 1: latin1 is an alias of ISO-8859-1
//! mb_strlen('x', 'nonsense');        // ValueError: must be a valid encoding
//! ```
//!
//! PHP 8.5 knows 79 encodings; Phera supports the 48 below. Every one of them
//! exists in PHP with the same aliases and MIME name, which a test checks.

use phera_extension_api::{Context, NativeError, Value, args};

use crate::state::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    SevenBit,
    EightBit,
    Ucs4,
    Ucs4Be,
    Ucs4Le,
    Ucs2,
    Ucs2Be,
    Ucs2Le,
    Utf32,
    Utf32Be,
    Utf32Le,
    Utf16,
    Utf16Be,
    Utf16Le,
    Utf8,
    Ascii,
    EucJp,
    Sjis,
    Cp932,
    SjisWin,
    Iso2022Jp,
    Gb18030,
    Windows1252,
    Windows1254,
    Iso8859_1,
    Iso8859_2,
    Iso8859_3,
    Iso8859_4,
    Iso8859_5,
    Iso8859_6,
    Iso8859_7,
    Iso8859_8,
    Iso8859_9,
    Iso8859_10,
    Iso8859_13,
    Iso8859_14,
    Iso8859_15,
    Iso8859_16,
    Cp936,
    Big5,
    EucKr,
    Windows1251,
    Koi8R,
    Koi8U,
    Base64,
    Uuencode,
    HtmlEntities,
    QuotedPrintable,
}

/// How an encoding maps bytes to code points, which decides the codec.
#[derive(Clone, Copy)]
pub enum Family {
    /// `7bit` and `ASCII`: a byte from 0x80 up is invalid.
    Ascii,
    /// `8bit`: every byte is the code point of the same number.
    Bytes,
    /// ISO-8859-*, Windows-125* and KOI8-*: PHP's 256 code points, four
    /// little-endian bytes each, `0xFFFFFFFF` for a byte with no character.
    SingleByte(&'static [u8; 1024]),
    /// UCS-4 and UTF-32: four bytes per character.
    Ucs4(ByteOrder),
    /// UCS-2: two bytes per character, so only the Basic Multilingual Plane.
    Ucs2(ByteOrder),
    /// UTF-16: two bytes, or four for a surrogate pair.
    Utf16(ByteOrder),
    Utf8,
    /// EUC-JP, the Shift_JIS variants, CP936, BIG-5 and EUC-KR, from tables
    /// generated with PHP.
    DoubleByte(&'static crate::codec::cjk::Tables),
    Iso2022Jp,
    Gb18030,
    /// Not character encodings but transfer formats, deprecated in PHP 8.2.
    Transfer(Transfer),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    Big,
    Little,
    /// `UTF-16`, `UTF-32`, `UCS-2`, `UCS-4`: big-endian, unless the string
    /// starts with a byte order mark that says otherwise.
    Marked,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Base64,
    Uuencode,
    HtmlEntities,
    QuotedPrintable,
}

/// What PHP calls an encoding: its name, its aliases and its MIME name.
struct Names {
    name: &'static str,
    aliases: &'static [&'static str],
    mime: &'static str,
}

const fn names(name: &'static str, aliases: &'static [&'static str], mime: &'static str) -> Names {
    Names {
        name,
        aliases,
        mime,
    }
}

impl Encoding {
    /// In the order `mb_list_encodings()` returns them.
    pub const ALL: [Encoding; 48] = [
        Self::SevenBit,
        Self::EightBit,
        Self::Ucs4,
        Self::Ucs4Be,
        Self::Ucs4Le,
        Self::Ucs2,
        Self::Ucs2Be,
        Self::Ucs2Le,
        Self::Utf32,
        Self::Utf32Be,
        Self::Utf32Le,
        Self::Utf16,
        Self::Utf16Be,
        Self::Utf16Le,
        Self::Utf8,
        Self::Ascii,
        Self::EucJp,
        Self::Sjis,
        Self::Cp932,
        Self::SjisWin,
        Self::Iso2022Jp,
        Self::Gb18030,
        Self::Windows1252,
        Self::Windows1254,
        Self::Iso8859_1,
        Self::Iso8859_2,
        Self::Iso8859_3,
        Self::Iso8859_4,
        Self::Iso8859_5,
        Self::Iso8859_6,
        Self::Iso8859_7,
        Self::Iso8859_8,
        Self::Iso8859_9,
        Self::Iso8859_10,
        Self::Iso8859_13,
        Self::Iso8859_14,
        Self::Iso8859_15,
        Self::Iso8859_16,
        Self::Cp936,
        Self::Big5,
        Self::EucKr,
        Self::Windows1251,
        Self::Koi8R,
        Self::Koi8U,
        Self::Base64,
        Self::Uuencode,
        Self::HtmlEntities,
        Self::QuotedPrintable,
    ];

    fn names(self) -> Names {
        match self {
            Self::SevenBit => names("7bit", &[], "7bit"),
            Self::EightBit => names("8bit", &["binary"], "8bit"),
            Self::Ucs4 => names("UCS-4", &["ISO-10646-UCS-4", "UCS4"], "UCS-4"),
            Self::Ucs4Be => names("UCS-4BE", &["byte4be"], "UCS-4BE"),
            Self::Ucs4Le => names("UCS-4LE", &["byte4le"], "UCS-4LE"),
            Self::Ucs2 => names("UCS-2", &["ISO-10646-UCS-2", "UCS2", "UNICODE"], "UCS-2"),
            Self::Ucs2Be => names("UCS-2BE", &["byte2be"], "UCS-2BE"),
            Self::Ucs2Le => names("UCS-2LE", &["byte2le"], "UCS-2LE"),
            Self::Utf32 => names("UTF-32", &["utf32"], "UTF-32"),
            Self::Utf32Be => names("UTF-32BE", &[], "UTF-32BE"),
            Self::Utf32Le => names("UTF-32LE", &[], "UTF-32LE"),
            Self::Utf16 => names("UTF-16", &["utf16"], "UTF-16"),
            Self::Utf16Be => names("UTF-16BE", &[], "UTF-16BE"),
            Self::Utf16Le => names("UTF-16LE", &[], "UTF-16LE"),
            Self::Utf8 => names("UTF-8", &["utf8"], "UTF-8"),
            Self::Ascii => names(
                "ASCII",
                &[
                    "ANSI_X3.4-1968",
                    "iso-ir-6",
                    "ANSI_X3.4-1986",
                    "ISO_646.irv:1991",
                    "US-ASCII",
                    "ISO646-US",
                    "us",
                    "IBM367",
                    "IBM-367",
                    "cp367",
                    "csASCII",
                ],
                "US-ASCII",
            ),
            Self::EucJp => names("EUC-JP", &["EUC", "EUC_JP", "eucJP", "x-euc-jp"], "EUC-JP"),
            Self::Sjis => names("SJIS", &["x-sjis", "SHIFT-JIS"], "Shift_JIS"),
            Self::Cp932 => names("CP932", &["MS932", "Windows-31J", "MS_Kanji"], "Shift_JIS"),
            Self::SjisWin => names("SJIS-win", &["SJIS-ms", "SJIS-open"], "Shift_JIS"),
            Self::Iso2022Jp => names("ISO-2022-JP", &[], "ISO-2022-JP"),
            Self::Gb18030 => names("GB18030", &["gb-18030", "gb-18030-2000"], "GB18030"),
            Self::Windows1252 => names("Windows-1252", &["cp1252"], "Windows-1252"),
            Self::Windows1254 => names(
                "Windows-1254",
                &["CP1254", "CP-1254", "WINDOWS-1254"],
                "Windows-1254",
            ),
            Self::Iso8859_1 => names("ISO-8859-1", &["ISO8859-1", "latin1"], "ISO-8859-1"),
            Self::Iso8859_2 => names("ISO-8859-2", &["ISO8859-2", "latin2"], "ISO-8859-2"),
            Self::Iso8859_3 => names("ISO-8859-3", &["ISO8859-3", "latin3"], "ISO-8859-3"),
            Self::Iso8859_4 => names("ISO-8859-4", &["ISO8859-4", "latin4"], "ISO-8859-4"),
            Self::Iso8859_5 => names("ISO-8859-5", &["ISO8859-5", "cyrillic"], "ISO-8859-5"),
            Self::Iso8859_6 => names("ISO-8859-6", &["ISO8859-6", "arabic"], "ISO-8859-6"),
            Self::Iso8859_7 => names("ISO-8859-7", &["ISO8859-7", "greek"], "ISO-8859-7"),
            Self::Iso8859_8 => names("ISO-8859-8", &["ISO8859-8", "hebrew"], "ISO-8859-8"),
            Self::Iso8859_9 => names("ISO-8859-9", &["ISO8859-9", "latin5"], "ISO-8859-9"),
            Self::Iso8859_10 => names("ISO-8859-10", &["ISO8859-10", "latin6"], "ISO-8859-10"),
            Self::Iso8859_13 => names("ISO-8859-13", &["ISO8859-13"], "ISO-8859-13"),
            Self::Iso8859_14 => names("ISO-8859-14", &["ISO8859-14", "latin8"], "ISO-8859-14"),
            Self::Iso8859_15 => names("ISO-8859-15", &["ISO8859-15"], "ISO-8859-15"),
            Self::Iso8859_16 => names("ISO-8859-16", &["ISO8859-16"], "ISO-8859-16"),
            Self::Cp936 => names("CP936", &["CP-936", "GBK"], "CP936"),
            Self::Big5 => names("BIG-5", &["CN-BIG5", "BIG-FIVE", "BIGFIVE"], "BIG5"),
            Self::EucKr => names("EUC-KR", &["EUC_KR", "eucKR", "x-euc-kr"], "EUC-KR"),
            Self::Windows1251 => names(
                "Windows-1251",
                &["CP1251", "CP-1251", "WINDOWS-1251"],
                "Windows-1251",
            ),
            Self::Koi8R => names("KOI8-R", &["KOI8R"], "KOI8-R"),
            Self::Koi8U => names("KOI8-U", &["KOI8U"], "KOI8-U"),
            Self::Base64 => names("BASE64", &[], "BASE64"),
            Self::Uuencode => names("UUENCODE", &[], "x-uuencode"),
            Self::HtmlEntities => names("HTML-ENTITIES", &["HTML", "html"], "HTML-ENTITIES"),
            Self::QuotedPrintable => names("Quoted-Printable", &["qprint"], "Quoted-Printable"),
        }
    }

    pub fn name(self) -> &'static str {
        self.names().name
    }

    pub fn aliases(self) -> &'static [&'static str] {
        self.names().aliases
    }

    pub fn mime_name(self) -> &'static str {
        self.names().mime
    }

    /// The encoding PHP finds under `name`: its own name, its MIME name or
    /// an alias, ignoring ASCII case. Several encodings share a MIME name
    /// (`Shift_JIS`); the first in [`Encoding::ALL`] wins, as in PHP.
    pub fn find(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|encoding| {
            let names = encoding.names();
            names.name.eq_ignore_ascii_case(name)
                || names.mime.eq_ignore_ascii_case(name)
                || names
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
        })
    }

    pub fn family(self) -> Family {
        use crate::codec::cjk;
        use ByteOrder::{Big, Little, Marked};
        match self {
            Self::SevenBit | Self::Ascii => Family::Ascii,
            Self::EightBit => Family::Bytes,
            Self::Ucs4 | Self::Utf32 => Family::Ucs4(Marked),
            Self::Ucs4Be | Self::Utf32Be => Family::Ucs4(Big),
            Self::Ucs4Le | Self::Utf32Le => Family::Ucs4(Little),
            Self::Ucs2 => Family::Ucs2(Marked),
            Self::Ucs2Be => Family::Ucs2(Big),
            Self::Ucs2Le => Family::Ucs2(Little),
            Self::Utf16 => Family::Utf16(Marked),
            Self::Utf16Be => Family::Utf16(Big),
            Self::Utf16Le => Family::Utf16(Little),
            Self::Utf8 => Family::Utf8,
            Self::EucJp => Family::DoubleByte(&cjk::EUC_JP),
            Self::Sjis => Family::DoubleByte(&cjk::SJIS),
            Self::Cp932 => Family::DoubleByte(&cjk::CP932),
            Self::SjisWin => Family::DoubleByte(&cjk::SJIS_WIN),
            Self::Cp936 => Family::DoubleByte(&cjk::CP936),
            Self::Big5 => Family::DoubleByte(&cjk::BIG5),
            Self::EucKr => Family::DoubleByte(&cjk::EUC_KR),
            Self::Iso2022Jp => Family::Iso2022Jp,
            Self::Gb18030 => Family::Gb18030,
            Self::Windows1252 => single(include_bytes!("../data/single-Windows-1252.bin")),
            Self::Windows1254 => single(include_bytes!("../data/single-Windows-1254.bin")),
            Self::Iso8859_1 => single(include_bytes!("../data/single-ISO-8859-1.bin")),
            Self::Iso8859_2 => single(include_bytes!("../data/single-ISO-8859-2.bin")),
            Self::Iso8859_3 => single(include_bytes!("../data/single-ISO-8859-3.bin")),
            Self::Iso8859_4 => single(include_bytes!("../data/single-ISO-8859-4.bin")),
            Self::Iso8859_5 => single(include_bytes!("../data/single-ISO-8859-5.bin")),
            Self::Iso8859_6 => single(include_bytes!("../data/single-ISO-8859-6.bin")),
            Self::Iso8859_7 => single(include_bytes!("../data/single-ISO-8859-7.bin")),
            Self::Iso8859_8 => single(include_bytes!("../data/single-ISO-8859-8.bin")),
            Self::Iso8859_9 => single(include_bytes!("../data/single-ISO-8859-9.bin")),
            Self::Iso8859_10 => single(include_bytes!("../data/single-ISO-8859-10.bin")),
            Self::Iso8859_13 => single(include_bytes!("../data/single-ISO-8859-13.bin")),
            Self::Iso8859_14 => single(include_bytes!("../data/single-ISO-8859-14.bin")),
            Self::Iso8859_15 => single(include_bytes!("../data/single-ISO-8859-15.bin")),
            Self::Iso8859_16 => single(include_bytes!("../data/single-ISO-8859-16.bin")),
            Self::Windows1251 => single(include_bytes!("../data/single-Windows-1251.bin")),
            Self::Koi8R => single(include_bytes!("../data/single-KOI8-R.bin")),
            Self::Koi8U => single(include_bytes!("../data/single-KOI8-U.bin")),
            Self::Base64 => Family::Transfer(Transfer::Base64),
            Self::Uuencode => Family::Transfer(Transfer::Uuencode),
            Self::HtmlEntities => Family::Transfer(Transfer::HtmlEntities),
            Self::QuotedPrintable => Family::Transfer(Transfer::QuotedPrintable),
        }
    }

    pub fn transfer(self) -> Option<Transfer> {
        match self.family() {
            Family::Transfer(transfer) => Some(transfer),
            _ => None,
        }
    }

    /// The number of bytes of every character, for encodings where that is
    /// fixed: PHP then counts and slices bytes without decoding them.
    pub fn fixed_width(self) -> Option<usize> {
        match self.family() {
            Family::Ascii | Family::Bytes | Family::SingleByte(_) => Some(1),
            Family::Ucs2(_) => Some(2),
            Family::Ucs4(_) => Some(4),
            _ => None,
        }
    }
}

fn single(table: &'static [u8; 1024]) -> Family {
    Family::SingleByte(table)
}

/// The encoding named by the optional argument at `index`, or the request's
/// internal encoding when it is left out or null.
pub fn argument(
    context: &mut dyn Context,
    arguments: &[Value],
    index: usize,
    function: &str,
) -> Result<Encoding, NativeError> {
    match args::string(arguments, index) {
        Some(name) => named(
            context,
            &name.to_string_lossy(),
            function,
            index + 1,
            "encoding",
        ),
        None => Ok(context.state().get_or_default::<Settings>().internal),
    }
}

/// The encoding `name` given for argument `number`, with the deprecation PHP
/// raises for a transfer encoding.
pub fn named(
    context: &mut dyn Context,
    name: &str,
    function: &str,
    number: usize,
    parameter: &str,
) -> Result<Encoding, NativeError> {
    let encoding = resolve(name, function, number, parameter)?;
    deprecate_transfer(context, name, encoding, function);
    Ok(encoding)
}

/// [`Encoding::find`], failing with PHP's `ValueError`.
pub fn resolve(
    name: &str,
    function: &str,
    number: usize,
    parameter: &str,
) -> Result<Encoding, NativeError> {
    Encoding::find(name).ok_or_else(|| {
        NativeError::value_error(format!(
            "{function}(): Argument #{number} (${parameter}) must be a valid encoding, \"{name}\" given"
        ))
    })
}

/// PHP deprecates the transfer encodings, but only says so when the name
/// differs from the previous one that a function was given:
///
/// ```php
/// mb_strlen('a', 'BASE64');   // Deprecated: Handling Base64 via mbstring is deprecated; ...
/// mb_strlen('a', 'base64');   // nothing: the same name, ignoring case
/// ```
pub fn deprecate_transfer(
    context: &mut dyn Context,
    name: &str,
    encoding: Encoding,
    function: &str,
) {
    let previous = &mut context.state().get_or_default::<Settings>().last_encoding;
    if previous
        .as_ref()
        .is_some_and(|previous| previous.eq_ignore_ascii_case(name))
    {
        return;
    }
    *previous = Some(name.into());
    let Some(transfer) = encoding.transfer() else {
        return;
    };
    let message = match transfer {
        Transfer::Base64 => {
            "Handling Base64 via mbstring is deprecated; use base64_encode/base64_decode instead"
        }
        Transfer::Uuencode => {
            "Handling Uuencode via mbstring is deprecated; \
             use convert_uuencode/convert_uudecode instead"
        }
        Transfer::HtmlEntities => {
            "Handling HTML entities via mbstring is deprecated; use htmlspecialchars, \
             htmlentities, or mb_encode_numericentity/mb_decode_numericentity instead"
        }
        Transfer::QuotedPrintable => {
            "Handling QPrint via mbstring is deprecated; use \
             quoted_printable_encode/quoted_printable_decode instead"
        }
    };
    context.deprecated(format!("{function}(): {message}"));
}

/// A little-endian `u32` at the start of `bytes`: the format of the tables
/// in `data/`.
pub fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
