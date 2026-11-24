//! Parse DBGp XML and canonicalize transport-only differences such as CDATA and attribute order.

use std::collections::BTreeMap;
use std::fmt::Write;

use quick_xml::escape::escape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Default)]
pub(crate) struct Normalize {
    breakpoints: BTreeMap<String, usize>,
}

impl Normalize {
    pub(crate) fn packet(&mut self, xml: &str, script: &str) -> String {
        let mut reader = Reader::from_str(xml);
        let mut output = String::new();
        let mut elements = Vec::new();
        loop {
            match reader.read_event().expect("DBGp XML should parse") {
                Event::Decl(_) => {}
                Event::Start(start) => {
                    let name = self.start(&mut output, start, script);
                    elements.push(name);
                }
                Event::Empty(start) => {
                    let name = self.start(&mut output, start, script);
                    write!(&mut output, "</{name}>").unwrap();
                }
                Event::End(end) => {
                    let name = elements.pop().expect("balanced XML");
                    assert_eq!(name, end.name().as_ref());
                    write!(&mut output, "</{name}>").unwrap();
                }
                Event::Text(value) if !elements.is_empty() => {
                    output.push_str(&escape(value.xml_content(XmlVersion::Implicit1_0)));
                }
                Event::CData(value) => {
                    output.push_str(&escape(value.xml_content(XmlVersion::Implicit1_0)));
                }
                Event::GeneralRef(reference) => {
                    output.push('&');
                    output.push_str(&reference);
                    output.push(';');
                }
                Event::Eof => break,
                _ => {}
            }
        }
        let output = output.replace(script, "{script}");
        if let Some((folder, _)) = script.rsplit_once(std::path::MAIN_SEPARATOR) {
            output.replace(folder, "{fixtures}")
        } else {
            output
        }
    }

    fn start(&mut self, output: &mut String, start: BytesStart<'_>, script: &str) -> String {
        let name = start.name().as_ref().to_owned();
        let mut attributes: BTreeMap<_, _> = start
            .attributes()
            .map(|attr| {
                let attr = attr.expect("well-formed attribute");
                let name = attr.key.as_ref().to_owned();
                let value = attr
                    .normalized_value(XmlVersion::Implicit1_0)
                    .expect("attribute escapes");
                (name, value.replace(script, "{script}"))
            })
            .collect();
        if name == "init" {
            attributes.remove("appid");
            attributes.remove("xdebug:language_version");
            if attributes.get("idekey").is_some_and(String::is_empty) {
                attributes.remove("idekey");
            }
        }
        let is_breakpoint = name == "breakpoint"
            || attributes
                .get("command")
                .is_some_and(|cmd| cmd == "breakpoint_set");
        if is_breakpoint && let Some(id) = attributes.get_mut("id") {
            let next = self.breakpoints.len() + 1;
            *id = self
                .breakpoints
                .entry(id.clone())
                .or_insert(next)
                .to_string();
        }
        write!(output, "<{name}").unwrap();
        for (key, value) in attributes {
            write!(output, r#" {key}="{}""#, escape(value)).unwrap();
        }
        output.push('>');
        name
    }
}
