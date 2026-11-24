//! DBGp properties encode byte strings in base64 and send a bounded page of children.

use std::fmt::Write;
use std::rc::Rc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_value::{LazyObject, LazyStrategy, Value};
use quick_xml::escape::escape;

use crate::Vm;

use super::features::Features;
use super::properties::{Property, scalar};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Identity {
    Array(usize),
    Object(u32),
}

enum Item {
    Open(Property, usize),
    Close,
}

pub(super) fn render(
    vm: &mut Vm,
    roots: Vec<Property>,
    features: &Features,
    page: usize,
) -> String {
    let mut xml = String::new();
    let mut pending: Vec<_> = roots
        .into_iter()
        .rev()
        .map(|root| Item::Open(root, 0))
        .collect();
    let mut ancestors = Vec::new();
    while let Some(item) = pending.pop() {
        let Item::Open(property, depth) = item else {
            xml.push_str("</property>");
            ancestors.pop();
            continue;
        };
        let (kind, bytes) = scalar(property.value.as_ref());
        let (identity, class, object_facet) = {
            let value = property
                .value
                .as_ref()
                .and_then(|value| value.try_deref().ok());
            match value.as_deref() {
                Some(Value::Array(array)) => (
                    Some(Identity::Array(Rc::as_ptr(array) as usize)),
                    None,
                    None,
                ),
                Some(Value::Object(object)) => {
                    let facet = match object.lazy().as_deref() {
                        Some(LazyObject::Pending {
                            strategy: LazyStrategy::Ghost,
                            ..
                        }) => Some("lazy-ghost"),
                        Some(LazyObject::Pending {
                            strategy: LazyStrategy::Proxy,
                            ..
                        }) => Some("lazy-proxy"),
                        _ if object.class.is_enum() => Some("enum"),
                        _ if object.class.name() == "Closure" => Some("closure"),
                        _ => None,
                    };
                    if object.is_uninitialized_lazy() && !features.show_hidden {
                        (None, None, facet)
                    } else {
                        (
                            Some(Identity::Object(object.id)),
                            Some(object.class.name().to_owned()),
                            facet,
                        )
                    }
                }
                _ => (None, None, None),
            }
        };
        let extended_value =
            features.extended_properties && kind == "string" && needs_base64(&bytes);
        let mut extended = String::new();
        let _ = write!(xml, r#"<property type="{kind}""#);
        if !property.name.is_empty() {
            name_field(
                &mut xml,
                &mut extended,
                "name",
                &property.name,
                features.extended_properties,
                extended_value,
            );
            name_field(
                &mut xml,
                &mut extended,
                "fullname",
                property.full.as_bytes(),
                features.extended_properties,
                extended_value,
            );
        }
        if let Some(class) = class {
            name_field(
                &mut xml,
                &mut extended,
                "classname",
                class.as_bytes(),
                features.extended_properties,
                false,
            );
        }
        let facet = [
            Some(property.facet.as_str()).filter(|facet| !facet.is_empty()),
            object_facet,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        if !facet.is_empty() {
            let _ = write!(xml, r#" facet="{}""#, escape(facet));
        }
        if let Some(identity) = identity {
            if ancestors.contains(&identity) {
                let _ = write!(xml, r#" children="1" recursive="1">{extended}</property>"#);
                continue;
            }
            let children = property.children(vm, features.show_hidden);
            let page = if depth == 0 { page } else { 0 };
            let count = children.len();
            let size = features.max_children;
            let _ = write!(
                xml,
                r#" children="{}" numchildren="{count}""#,
                u8::from(count != 0)
            );
            if depth < features.max_depth {
                let _ = write!(xml, r#" page="{page}" pagesize="{size}""#);
            }
            xml.push('>');
            xml.push_str(&extended);
            pending.push(Item::Close);
            ancestors.push(identity);
            if depth < features.max_depth {
                let slice: Vec<_> = children
                    .into_iter()
                    .skip(size.saturating_mul(page))
                    .take(size)
                    .collect();
                pending.extend(
                    slice
                        .into_iter()
                        .rev()
                        .map(|child| Item::Open(child, depth + 1)),
                );
            }
        } else {
            if kind == "string" {
                let size = bytes.len();
                let len = if features.max_data == 0 {
                    size
                } else {
                    size.min(features.max_data)
                };
                let encoded = STANDARD.encode(&bytes[..len]);
                if extended_value {
                    let _ = write!(
                        xml,
                        r#" size="{size}">{extended}<value encoding="base64">{encoded}</value>"#
                    );
                } else {
                    let _ = write!(
                        xml,
                        r#" size="{size}" encoding="base64">{extended}{encoded}"#
                    );
                }
            } else {
                xml.push('>');
                xml.push_str(&extended);
                xml.push_str(&escape(String::from_utf8_lossy(&bytes)));
            }
            xml.push_str("</property>");
        }
    }
    xml
}

fn name_field(
    xml: &mut String,
    extended: &mut String,
    field: &str,
    bytes: &[u8],
    extended_properties: bool,
    force_base64: bool,
) {
    if extended_properties && (force_base64 || needs_base64(bytes)) {
        let _ = write!(
            extended,
            r#"<{field} encoding="base64">{}</{field}>"#,
            STANDARD.encode(bytes)
        );
    } else {
        let text = String::from_utf8_lossy(bytes).replace(char::is_control, "\u{fffd}");
        let _ = write!(xml, r#" {field}="{}""#, escape(text));
    }
}

fn needs_base64(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_err() || bytes.iter().any(u8::is_ascii_control)
}
