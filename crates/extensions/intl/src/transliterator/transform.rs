//! Translate ICU transform IDs to the identifiers in ICU4X's compiled data.

use std::rc::Rc;

use icu_casemap::CaseMapper;
use icu_experimental::transliterate::Transliterator;
use icu_locale::LanguageIdentifier;
use icu_normalizer::{ComposingNormalizer, DecomposingNormalizer};
use icu_properties::{
    CodePointMapData, PropertyParser,
    props::{GeneralCategory, GeneralCategoryGroup, Script},
};

const SCRIPTS: &[(Script, &str, &str)] = &[
    (Script::Arabic, "Arabic", "arab"),
    (Script::Armenian, "Armenian", "armn"),
    (Script::Bengali, "Bengali", "beng"),
    (Script::Bopomofo, "Bopomofo", "bopo"),
    (Script::Cyrillic, "Cyrillic", "cyrl"),
    (Script::Devanagari, "Devanagari", "deva"),
    (Script::Georgian, "Georgian", "geor"),
    (Script::Greek, "Greek", "grek"),
    (Script::Gujarati, "Gujarati", "gujr"),
    (Script::Gurmukhi, "Gurmukhi", "guru"),
    (Script::Hangul, "Hangul", "hang"),
    (Script::Han, "Han", "hans"),
    (Script::Hebrew, "Hebrew", "hebr"),
    (Script::Hiragana, "Hiragana", "hira"),
    (Script::Kannada, "Kannada", "knda"),
    (Script::Katakana, "Katakana", "kana"),
    (Script::Malayalam, "Malayalam", "mlym"),
    (Script::Oriya, "Oriya", "orya"),
    (Script::Tamil, "Tamil", "taml"),
    (Script::Telugu, "Telugu", "telu"),
];

const LANGUAGES: &[(&str, &str)] = &[
    ("Amharic", "am"),
    ("Arabic", "ar"),
    ("Belarusian", "be"),
    ("Bulgarian", "bg"),
    ("Greek", "el"),
    ("Persian", "fa"),
    ("Hebrew", "he"),
    ("Armenian", "hy"),
    ("Georgian", "ka"),
    ("Kazakh", "kk"),
    ("Korean", "ko"),
    ("Kirghiz", "ky"),
    ("Macedonian", "mk"),
    ("Mongolian", "mn"),
    ("Pashto", "ps"),
    ("Russian", "ru"),
    ("Serbian", "sr"),
    ("Ukrainian", "uk"),
];

type ScriptTransforms = Vec<(Script, Rc<Transliterator>)>;

#[derive(Clone)]
pub enum Step {
    Rule(Rc<Transliterator>),
    Scripts(Rc<ScriptTransforms>),
    Lower,
    Upper,
    Nfc,
    Nfd,
    Nfkc,
    Nfkd,
    Remove,
    RemoveCategory(GeneralCategoryGroup, bool),
    Null,
}

impl Step {
    pub fn load(id: &str) -> Option<Self> {
        let id = id.to_ascii_lowercase();
        if let Some(filter) = id.strip_prefix("[:") {
            let (property, transform) = filter.split_once(":]")?;
            if !matches!(transform.trim(), "remove" | "any-remove") {
                return None;
            }
            let property = property.trim();
            let inverted = property.starts_with('^');
            let category = property.trim_start_matches('^');
            let category = PropertyParser::<GeneralCategoryGroup>::new().get_loose(category)?;
            return Some(Self::RemoveCategory(category, inverted));
        }
        let id = id.strip_prefix("any-").unwrap_or(&id);
        Some(match id {
            "lower" | "lower()" => Self::Lower,
            "upper" | "upper()" => Self::Upper,
            "nfc" => Self::Nfc,
            "nfd" => Self::Nfd,
            "nfkc" => Self::Nfkc,
            "nfkd" => Self::Nfkd,
            "remove" => Self::Remove,
            "null" => Self::Null,
            "latin" => Self::Scripts(Rc::new(
                SCRIPTS
                    .iter()
                    .filter_map(|&(script, _, tag)| {
                        Some((script, compiled(&format!("und-Latn-t-und-{tag}"))?))
                    })
                    .collect(),
            )),
            "latin-ascii" => Self::Rule(compiled("und-t-und-latn-d0-ascii")?),
            "de-ascii" => Self::Rule(compiled("de-t-de-d0-ascii")?),
            "fullwidth-halfwidth" => Self::Rule(compiled("und-t-d0-hwidth")?),
            "halfwidth-fullwidth" => Self::Rule(compiled("und-t-d0-fwidth")?),
            _ => {
                let (name, target) = id.split_once('-')?;
                if target == "latin/bgn" {
                    Self::Rule(bgn(name)?)
                } else {
                    let tag = |id: &str| {
                        if id == "latin" {
                            Some("latn")
                        } else {
                            SCRIPTS
                                .iter()
                                .find(|(_, key, _)| id.eq_ignore_ascii_case(key))
                                .map(|(_, _, tag)| *tag)
                        }
                    };
                    Self::Rule(compiled(&format!(
                        "und-{}-t-und-{}",
                        tag(target)?,
                        tag(name)?
                    ))?)
                }
            }
        })
    }

    pub fn apply(&self, mut input: String) -> String {
        match self {
            Self::Rule(rule) => rule.transliterate(input),
            Self::Scripts(rules) => script_runs(&input, rules),
            Self::Lower => CaseMapper::new()
                .lowercase_to_string(&input, &LanguageIdentifier::UNKNOWN)
                .into_owned(),
            Self::Upper => CaseMapper::new()
                .uppercase_to_string(&input, &LanguageIdentifier::UNKNOWN)
                .into_owned(),
            Self::Nfc => ComposingNormalizer::new_nfc()
                .normalize(&input)
                .into_owned(),
            Self::Nfd => DecomposingNormalizer::new_nfd()
                .normalize(&input)
                .into_owned(),
            Self::Nfkc => ComposingNormalizer::new_nfkc()
                .normalize(&input)
                .into_owned(),
            Self::Nfkd => DecomposingNormalizer::new_nfkd()
                .normalize(&input)
                .into_owned(),
            Self::Remove => String::new(),
            Self::RemoveCategory(category, inverted) => {
                let categories = CodePointMapData::<GeneralCategory>::new();
                input.retain(|ch| category.contains(categories.get(ch)) == *inverted);
                input
            }
            Self::Null => input,
        }
    }
}

fn compiled(tag: &str) -> Option<Rc<Transliterator>> {
    Transliterator::try_new(&tag.parse().ok()?)
        .ok()
        .map(Rc::new)
}

fn bgn(name: &str) -> Option<Rc<Transliterator>> {
    let (_, language) = LANGUAGES
        .iter()
        .find(|(key, _)| name.eq_ignore_ascii_case(key))?;
    compiled(&format!("{language}-Latn-t-{language}-m0-bgn"))
}

fn script_runs(input: &str, rules: &ScriptTransforms) -> String {
    let scripts = CodePointMapData::<Script>::new();
    let mut output = String::with_capacity(input.len());
    let mut active = Script::Unknown;
    let mut start = 0;
    let flush = |output: &mut String, script, text: &str| {
        if let Some((_, rule)) = rules.iter().find(|(key, _)| *key == script) {
            output.push_str(&rule.transliterate(text.to_owned()));
        } else {
            output.push_str(text);
        }
    };
    for (offset, ch) in input.char_indices() {
        let script = scripts.get(ch);
        if script != active && !matches!(script, Script::Inherited | Script::Common) {
            flush(&mut output, active, &input[start..offset]);
            active = script;
            start = offset;
        }
    }
    flush(&mut output, active, &input[start..]);
    output
}

pub fn ids() -> impl Iterator<Item = String> {
    [
        "Any-Latin",
        "Latin-ASCII",
        "Lower",
        "Upper",
        "NFC",
        "NFD",
        "NFKC",
        "NFKD",
        "Null",
        "Fullwidth-Halfwidth",
        "Halfwidth-Fullwidth",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(SCRIPTS.iter().map(|(_, name, _)| format!("{name}-Latin")))
    .chain(SCRIPTS.iter().map(|(_, name, _)| format!("Latin-{name}")))
    .chain(
        LANGUAGES
            .iter()
            .map(|(name, _)| format!("{name}-Latin/BGN")),
    )
    .filter(|id| Step::load(id).is_some())
}
