//! Optional IDNA context and bidirectional checks use Unicode properties.
//!
//! ```php
//! idn_to_ascii("a\u{200d}b.com", IDNA_DEFAULT | IDNA_CHECK_CONTEXTJ); // false
//! ```

use icu_normalizer::uts46::Uts46Mapper;
use icu_properties::{
    CodePointMapData,
    props::{BidiClass, JoiningType},
};

pub fn has_rtl(label: &str) -> bool {
    let bidi = CodePointMapData::<BidiClass>::new();
    label.chars().map(|scalar| bidi.get(scalar)).any(|class| {
        matches!(
            class,
            BidiClass::RightToLeft | BidiClass::ArabicLetter | BidiClass::ArabicNumber
        )
    })
}

pub fn valid_bidi(label: &str) -> bool {
    use BidiClass as B;
    let data = CodePointMapData::<B>::new();
    let mut classes = label.chars().map(|scalar| data.get(scalar));
    let Some(first) = classes.next() else {
        return false;
    };
    let rtl = matches!(first, B::RightToLeft | B::ArabicLetter);
    if !rtl && first != B::LeftToRight {
        return false;
    }
    let mut trailing = first;
    let mut arabic_numbers = false;
    let mut european_numbers = false;
    for class in classes {
        match class {
            B::EuropeanNumber => european_numbers = true,
            B::ArabicNumber if rtl => arabic_numbers = true,
            B::RightToLeft | B::ArabicLetter if rtl => {}
            B::LeftToRight if !rtl => {}
            B::EuropeanSeparator
            | B::EuropeanTerminator
            | B::CommonSeparator
            | B::OtherNeutral
            | B::BoundaryNeutral
            | B::NonspacingMark => {}
            _ => return false,
        }
        if class != B::NonspacingMark {
            trailing = class;
        }
    }
    if rtl {
        !(arabic_numbers && european_numbers)
            && matches!(
                trailing,
                B::RightToLeft | B::ArabicLetter | B::EuropeanNumber | B::ArabicNumber
            )
    } else {
        matches!(trailing, B::LeftToRight | B::EuropeanNumber)
    }
}

pub fn valid_context_j(label: &str) -> bool {
    use JoiningType as J;
    let mapper = Uts46Mapper::new();
    let joining = CodePointMapData::<J>::new();
    let mut previous = None;
    let mut previous_joining = J::NonJoining;
    for (index, scalar) in label.char_indices() {
        if matches!(scalar, '\u{200c}' | '\u{200d}')
            && !previous.is_some_and(|previous| mapper.is_virama(previous))
        {
            if scalar == '\u{200d}' || !matches!(previous_joining, J::LeftJoining | J::DualJoining)
            {
                return false;
            }
            let following = label[index + scalar.len_utf8()..]
                .chars()
                .map(|next| joining.get(next))
                .find(|next| *next != J::Transparent);
            if !matches!(following, Some(J::RightJoining | J::DualJoining)) {
                return false;
            }
        }
        previous = Some(scalar);
        let class = joining.get(scalar);
        if class != J::Transparent {
            previous_joining = class;
        }
    }
    true
}
