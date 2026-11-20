//! `property_get -i 8 -n "$items" -p 1` reads a page of an array or object's children.

use std::fmt::Write;
use std::rc::Rc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_extension_api::{Context, PropertyPurpose, PropertyView};
use phera_value::{ArrayKey, Object, Value};
use quick_xml::escape::escape;

use crate::Vm;

use super::features::Features;

pub(super) struct Property {
    name: Vec<u8>,
    pub(super) full: String,
    pub(super) value: Option<Value>,
    facet: String,
}

impl Property {
    pub(super) fn new(name: String, value: Option<Value>) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            full: name,
            value,
            facet: String::new(),
        }
    }

    pub(super) fn with_facet(mut self, facet: &str) -> Self {
        self.facet = facet.to_owned();
        self
    }

    pub(super) fn contents(&self, max: usize) -> (usize, String) {
        let (_, value) = scalar(self.value.as_ref());
        let size = value.len();
        let available = if max == 0 { size } else { size.min(max) };
        (size, STANDARD.encode(&value[..available]))
    }

    pub(super) fn find(self, vm: &mut Vm, full: &str) -> Option<Self> {
        let mut pending = vec![self];
        while let Some(property) = pending.pop() {
            if full == property.full {
                return Some(property);
            }
            if full.strip_prefix(&property.full).is_some_and(|tail| {
                tail.starts_with('[') || tail.starts_with("->") || tail.starts_with("::")
            }) {
                pending.extend(property.children(vm));
            }
        }
        None
    }

    fn children(&self, vm: &mut Vm) -> Vec<Self> {
        let value = self.value.as_ref().and_then(|value| value.try_deref().ok());
        match value.as_deref() {
            Some(Value::Array(array)) => array
                .iter()
                .map(|(key, value)| {
                    let (name, index) = array_key(&key);
                    Self {
                        name,
                        full: format!("{}[{index}]", self.full),
                        value: Some(value.clone()),
                        facet: String::new(),
                    }
                })
                .collect(),
            Some(Value::Object(object)) => self.object_children(vm, object),
            _ => Vec::new(),
        }
    }

    fn object_children(&self, vm: &mut Vm, object: &Rc<Object>) -> Vec<Self> {
        let class = vm.classes.for_object(object);
        let has_projection = class
            .as_ref()
            .and_then(|class| class.hooks.as_ref())
            .is_some_and(|hooks| hooks.properties.is_some());
        let (slots, uninitialized) = if has_projection && !object.is_uninitialized_lazy() {
            let Ok(slots) = Context::object_properties(vm, object, PropertyPurpose::Debug) else {
                return Vec::new();
            };
            (Rc::new(slots), Default::default())
        } else if let Some(class) = &class
            && let PropertyView::DebugSlots {
                slots,
                uninitialized,
            } = vm.dump_hooked_slots(object, class)
        {
            (slots, uninitialized)
        } else {
            (Rc::new(object.to_array()), Default::default())
        };
        let mut children: Vec<_> = slots
            .iter()
            .map(|(key, value)| {
                let property = class
                    .as_ref()
                    .and_then(|class| class.defaults.iter().find(|p| p.key == key));
                let (name, member, facet) = object_key(&key, object.class.name());
                let mut child = Self {
                    name,
                    full: format!("{}->{member}", self.full),
                    value: (!uninitialized.contains_key(&key)).then(|| value.clone()),
                    facet: facet.to_owned(),
                };
                if property.is_some_and(|property| property.is_readonly) {
                    child.facet.push_str(" readonly");
                }
                child
            })
            .collect();
        if let Some(class) = class {
            for property in &class.static_properties {
                if let Some(storage) = &property.static_storage {
                    let value = storage
                        .reference
                        .try_borrow()
                        .ok()
                        .map(|value| Value::Reference(value.clone()));
                    let (name, member, facet) = object_key(&property.key, &class.name);
                    children.push(Self {
                        name,
                        full: format!("{}::${member}", self.full),
                        value: value.filter(|_| property.value_initialized.get()),
                        facet: format!("static {facet}"),
                    });
                }
            }
        }
        children
    }
}

fn array_key(key: &ArrayKey) -> (Vec<u8>, String) {
    match key {
        ArrayKey::Int(number) => (number.to_string().into_bytes(), number.to_string()),
        ArrayKey::String(text) => (
            text.as_bytes().to_vec(),
            format!("'{}'", quote(text.as_bytes(), '\'')),
        ),
    }
}

fn object_key(key: &ArrayKey, class: &str) -> (Vec<u8>, String, &'static str) {
    let ArrayKey::String(text) = key else {
        let (name, member) = array_key(key);
        return (name, member, "public");
    };
    let mut parts = text.as_bytes().splitn(3, |&byte| byte == 0);
    let first = parts.next().unwrap_or_default();
    let (name, name_in_path, visibility) = match (first, parts.next(), parts.next()) {
        (b"", Some(b"*"), Some(name)) => (name, quote(name, '"'), "protected"),
        (b"", Some(owner), Some(name)) if owner == class.as_bytes() => {
            (name, quote(name, '"'), "private")
        }
        (b"", Some(owner), Some(name)) => (
            name,
            format!("*{}*{}", String::from_utf8_lossy(owner), quote(name, '"')),
            "private",
        ),
        _ => (text.as_bytes(), quote(text.as_bytes(), '"'), "public"),
    };
    let member = if name_in_path
        .chars()
        .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '*' | '\\'))
    {
        name_in_path
    } else {
        format!("{{\"{name_in_path}\"}}")
    };
    (name.to_vec(), member, visibility)
}

fn quote(bytes: &[u8], delimiter: char) -> String {
    let mut result = String::new();
    let source = String::from_utf8_lossy(bytes);
    for ch in source.chars() {
        if ch == delimiter || ch == '\\' {
            result.push('\\');
        }
        if ch.is_control() {
            let _ = write!(result, "\\x{:02X}", ch as u32);
        } else {
            result.push(ch);
        }
    }
    result
}

fn scalar(value: Option<&Value>) -> (&'static str, Vec<u8>) {
    let value = value.and_then(|value| value.try_deref().ok());
    match value.as_deref() {
        Some(Value::Null) => ("null", Vec::new()),
        Some(Value::Bool(value)) => ("bool", vec![if *value { b'1' } else { b'0' }]),
        Some(Value::Int(value)) => ("int", value.to_string().into_bytes()),
        Some(Value::Float(value)) => ("float", phera_value::float_repr(*value).into_bytes()),
        Some(Value::String(value)) => ("string", value.as_bytes().to_vec()),
        Some(Value::Resource(value)) => (
            "resource",
            format!("resource id='{}' type='{}'", value.id, value.kind).into_bytes(),
        ),
        Some(Value::Array(_)) => ("array", Vec::new()),
        Some(Value::Object(_)) => ("object", Vec::new()),
        _ => ("uninitialized", Vec::new()),
    }
}

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
        let (identity, class) = {
            let value = property
                .value
                .as_ref()
                .and_then(|value| value.try_deref().ok());
            match value.as_deref() {
                Some(Value::Array(array)) => {
                    (Some(Identity::Array(Rc::as_ptr(array) as usize)), None)
                }
                Some(Value::Object(object)) => (
                    Some(Identity::Object(object.id)),
                    Some(object.class.name().to_owned()),
                ),
                _ => (None, None),
            }
        };
        let mut extended = String::new();
        let _ = write!(xml, r#"<property type="{kind}""#);
        if !property.name.is_empty() {
            name_field(&mut xml, &mut extended, "name", &property.name, features);
            name_field(
                &mut xml,
                &mut extended,
                "fullname",
                property.full.as_bytes(),
                features,
            );
        }
        if let Some(class) = class {
            name_field(
                &mut xml,
                &mut extended,
                "classname",
                class.as_bytes(),
                features,
            );
        }
        if !property.facet.is_empty() {
            let _ = write!(xml, r#" facet="{}""#, escape(&property.facet));
        }
        if let Some(identity) = identity {
            let recursive = ancestors.contains(&identity);
            let children = if recursive {
                Vec::new()
            } else {
                property.children(vm)
            };
            let page = if depth == 0 { page } else { 0 };
            let count = children.len();
            let size = features.max_children;
            let _ = write!(
                xml,
                r#" children="{}" numchildren="{count}" page="{page}" pagesize="{size}""#,
                u8::from(count != 0)
            );
            if recursive {
                xml.push_str(r#" recursive="1""#);
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
                let _ = write!(
                    xml,
                    r#" size="{size}" encoding="base64">{extended}{}"#,
                    STANDARD.encode(&bytes[..len])
                );
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
    features: &Features,
) {
    if features.extended_properties
        && (std::str::from_utf8(bytes).is_err() || bytes.iter().any(u8::is_ascii_control))
    {
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
