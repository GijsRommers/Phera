//! `property_get -i 8 -n "$items" -p 1` reads a page of an array or object's children.

use std::fmt::Write;
use std::rc::Rc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_extension_api::{Context, PropertyPurpose, PropertyView};
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;

use super::features::Features;

pub(super) struct Property {
    pub(super) name: Vec<u8>,
    pub(super) full: String,
    pub(super) value: Option<Value>,
    pub(super) facet: String,
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

    pub(super) fn member(full: &str, name: &str, value: Value) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            full: format!("{full}->{name}"),
            value: Some(value),
            facet: "public".into(),
        }
    }

    pub(super) fn with_facet(mut self, facet: &str) -> Self {
        self.facet = facet.to_owned();
        self
    }

    pub(super) fn named(mut self, full: &str) -> Self {
        self.name = full.as_bytes().to_vec();
        self.full = full.to_owned();
        self
    }

    pub(super) fn contents(&self, max: usize) -> (&'static str, usize, String) {
        let (kind, value) = scalar(self.value.as_ref());
        let size = value.len();
        let available = if max == 0 { size } else { size.min(max) };
        (kind, size, STANDARD.encode(&value[..available]))
    }

    pub(super) fn find(self, vm: &mut Vm, full: &str, features: &Features) -> Option<Self> {
        let alternate = full.replace("['", "[\"").replace("']", "\"]");
        let mut pending = vec![self];
        while let Some(property) = pending.pop() {
            if full == property.full || alternate == property.full {
                return Some(property);
            }
            let below = |name: &str| {
                name.strip_prefix(&property.full).is_some_and(|tail| {
                    tail.starts_with('[') || tail.starts_with("->") || tail.starts_with("::")
                })
            };
            if below(full) || below(&alternate) {
                pending.extend(property.children(vm, features.show_hidden));
            }
        }
        None
    }

    pub(super) fn children(&self, vm: &mut Vm, show_hidden: bool) -> Vec<Self> {
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
            Some(Value::Object(object)) => self.object_children(vm, object, show_hidden),
            _ => Vec::new(),
        }
    }

    fn object_children(&self, vm: &mut Vm, object: &Rc<Object>, show_hidden: bool) -> Vec<Self> {
        if object.class.name() == "Closure" {
            return super::closure::children(vm, object, &self.full);
        }
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
        let mut members: Vec<_> = slots
            .iter()
            .filter(|(key, _)| show_hidden || !uninitialized.contains_key(key))
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
                if object.class.is_enum() || property.is_some_and(|property| property.is_readonly) {
                    child.facet.push_str(" readonly");
                }
                child
            })
            .collect();
        let mut children = Vec::new();
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
                        full: format!("{}::{member}", self.full),
                        value: value.filter(|_| property.value_initialized.get()),
                        facet: format!("static {facet}"),
                    });
                }
            }
        }
        children.append(&mut members);
        children
    }
}

fn array_key(key: &ArrayKey) -> (Vec<u8>, String) {
    match key {
        ArrayKey::Int(number) => (number.to_string().into_bytes(), number.to_string()),
        ArrayKey::String(text) => (
            text.as_bytes().to_vec(),
            format!("\"{}\"", quote(text.as_bytes(), '"')),
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

pub(super) fn scalar(value: Option<&Value>) -> (&'static str, Vec<u8>) {
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
