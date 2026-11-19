//! Verify input safety and lifetime-independent arena operations.

use phera_ext_dom::{serialize, tree::Arena, xml};

#[test]
fn external_entities_never_expand() {
    let source = br#"<!DOCTYPE root [
        <!ENTITY external SYSTEM "file:///this-file-does-not-exist">
    ]><root>&external;</root>"#;
    let parsed = xml::parse(
        source,
        xml::Options {
            substitute_entities: true,
            ..xml::Options::default()
        },
    );
    assert!(parsed.diagnostics.is_empty());
    let document = parsed.document.expect("the external entity was left empty");
    let xml = serialize::save(&document, Arena::DOCUMENT, serialize::Options::default());
    assert!(xml.ends_with(b"<root/>\n"));
}

#[test]
fn entity_limit_cannot_be_relaxed_by_recover_or_huge() {
    let mut source = String::from("<!DOCTYPE root [<!ENTITY a0 'some content'>");
    for index in 1..9 {
        let reference = format!("&a{};", index - 1).repeat(10);
        source.push_str(&format!("<!ENTITY a{index} '{reference}'>"));
    }
    source.push_str("]><root>&a8;</root>");
    let parsed = xml::parse(
        source.as_bytes(),
        xml::Options {
            substitute_entities: true,
            recover: true,
            huge: true,
            ..xml::Options::default()
        },
    );
    assert!(parsed.document.is_none());
    assert!(parsed.diagnostics.iter().any(|error| error.code == 89));
}

#[test]
fn deeply_nested_xml_can_be_serialized_iteratively() {
    let depth = 12_000;
    let source = format!("{}{}", "<r>".repeat(depth), "</r>".repeat(depth));
    let parsed = xml::parse(
        source.as_bytes(),
        xml::Options {
            huge: true,
            ..xml::Options::default()
        },
    );
    let document = parsed.document.expect("nested XML");
    assert_eq!(document.arena.descendants(Arena::DOCUMENT).count(), depth);
    let xml = serialize::save(&document, Arena::DOCUMENT, serialize::Options::default());
    assert_eq!(
        xml.len(),
        source.len() + b"<?xml version=\"1.0\"?>\n\n".len() - 3
    );
}
