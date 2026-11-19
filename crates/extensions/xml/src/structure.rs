//! Build xml_parse_into_struct's parallel value and tag-index arrays.
//!
//! ```php
//! xml_parse_into_struct(xml_parser_create(), '<r><item id="1"/></r>', $rows, $index);
//! echo $rows[1]['type']; // complete
//! ```

use phera_extension_api::{ArrayKey, PhpArray, PhpString, Value};

use crate::events::{Emission, Handler};

#[derive(Default)]
pub struct Structure {
    values: Vec<Row>,
    parents: Vec<usize>,
    over_depth: usize,
}

struct Row {
    name: PhpString,
    kind: &'static str,
    level: usize,
    attributes: Option<Value>,
    text: Vec<u8>,
}

impl Structure {
    /// Return whether this event crosses the PHP struct output depth limit.
    pub fn observe(&mut self, event: &Emission, skip_white: bool) -> bool {
        const MAX_LEVEL: usize = 255;
        match event.handler {
            Handler::Start if self.parents.len() == MAX_LEVEL => {
                self.over_depth += 1;
                return self.over_depth == 1;
            }
            Handler::End if self.over_depth > 0 => self.over_depth -= 1,
            _ if self.over_depth > 0 => {}
            Handler::Start => self.start(event),
            Handler::End => self.end(event),
            Handler::Text => self.text(event, skip_white),
            _ => {}
        }
        false
    }

    fn start(&mut self, event: &Emission) {
        let [Value::String(name), attributes, ..] = event.data.as_slice() else {
            return;
        };
        let level = self.parents.len() + 1;
        self.parents.push(self.values.len());
        self.values.push(Row {
            name: name.clone(),
            kind: "open",
            level,
            attributes: (!attributes.is_array() || php_array_has_entries(attributes))
                .then(|| attributes.clone()),
            text: Vec::new(),
        });
    }

    fn end(&mut self, event: &Emission) {
        let Some(start) = self.parents.pop() else {
            return;
        };
        if start + 1 == self.values.len() {
            self.values[start].kind = "complete";
        } else if let Some(Value::String(name)) = event.data.first() {
            self.values.push(Row {
                name: name.clone(),
                kind: "close",
                level: self.parents.len() + 1,
                attributes: None,
                text: Vec::new(),
            });
        }
    }

    fn text(&mut self, event: &Emission, skip_white: bool) {
        let Some(Value::String(text)) = event.data.first() else {
            return;
        };
        if skip_white && text.as_bytes().iter().all(u8::is_ascii_whitespace) {
            return;
        }
        let Some(&start) = self.parents.last() else {
            return;
        };
        let level = self.parents.len();
        if start + 1 == self.values.len()
            || self
                .values
                .last()
                .is_some_and(|row| row.kind == "cdata" && row.level == level)
        {
            if let Some(row) = self.values.last_mut() {
                row.text.extend_from_slice(text.as_bytes());
            }
        } else {
            self.values.push(Row {
                name: self.values[start].name.clone(),
                kind: "cdata",
                level,
                attributes: None,
                text: text.as_bytes().to_vec(),
            });
        }
    }

    pub fn finish(self) -> (Value, Value) {
        let mut rows = PhpArray::new();
        let mut positions = indexmap::IndexMap::<PhpString, PhpArray>::new();
        for (index, mut row) in self.values.into_iter().enumerate() {
            let list = positions.entry(row.name.clone()).or_default();
            let _ = list.push(Value::Int(index as i64));
            let mut data = PhpArray::new();
            data.insert(ArrayKey::str("tag"), Value::String(row.name));
            if row.kind == "cdata" {
                data.insert(
                    ArrayKey::str("value"),
                    Value::from(std::mem::take(&mut row.text)),
                );
            }
            data.insert(ArrayKey::str("type"), Value::from(row.kind));
            data.insert(ArrayKey::str("level"), Value::Int(row.level as i64));
            if let Some(attributes) = row.attributes {
                data.insert(ArrayKey::str("attributes"), attributes);
            }
            if !row.text.is_empty() {
                data.insert(ArrayKey::str("value"), Value::from(row.text));
            }
            let _ = rows.push(Value::from(data));
        }
        let mut index = PhpArray::new();
        for (name, values) in positions {
            index.insert(ArrayKey::str(name), Value::from(values));
        }
        (Value::from(rows), Value::from(index))
    }
}

fn php_array_has_entries(value: &Value) -> bool {
    matches!(value, Value::Array(values) if !values.is_empty())
}
