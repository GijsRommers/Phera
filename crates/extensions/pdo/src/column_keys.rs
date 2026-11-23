//! Column keys shared by every row of a result such as `SELECT id AS "postId"`.

use std::cell::OnceCell;

use phera_extension_api::{ArrayKey, PhpString};

use crate::attributes::{Attributes, ColumnCase};
use crate::drivers::Column;
use crate::metadata::Metadata;

#[derive(Default)]
pub struct ColumnKeys {
    naming: Option<Naming>,
    keys: OnceCell<Described>,
}

struct Described {
    keys: Vec<Key>,
    distinct: bool,
}

#[derive(Clone, Copy, Default)]
pub struct Layout<'a> {
    pub keys: &'a [Key],
    pub distinct: bool,
}

impl<'a> Layout<'a> {
    pub fn without_first(self) -> Self {
        Self {
            keys: self.keys.get(1..).unwrap_or_default(),
            ..self
        }
    }
}

#[derive(Clone, Copy)]
struct Naming {
    case: ColumnCase,
    table_names: bool,
}

pub struct Key {
    pub array: ArrayKey,
    pub property: ArrayKey,
}

impl ColumnKeys {
    pub fn describe(&mut self, columns: &[Column], attributes: &Attributes) {
        if !columns.is_empty() {
            self.naming.get_or_insert(Naming {
                case: attributes.case,
                table_names: attributes.mysql_table_names,
            });
        }
        self.keys.take();
    }

    pub fn get(&self, columns: &[Column]) -> Layout<'_> {
        let Some(naming) = self.naming else {
            return Layout::default();
        };
        let described = self.keys.get_or_init(|| {
            let keys: Vec<_> = columns
                .iter()
                .map(|column| {
                    let name = PhpString::from(naming.name(column));
                    Key {
                        array: ArrayKey::from_string(name.clone()),
                        property: ArrayKey::String(name),
                    }
                })
                .collect();
            // PHP arrays already scan small maps. Bound this scan to the
            // same width so a wide result does not need a quadratic check.
            let distinct = keys.len() <= 8
                && keys.iter().enumerate().all(|(index, key)| {
                    !keys[..index]
                        .iter()
                        .any(|earlier| earlier.array == key.array)
                });
            Described { keys, distinct }
        });
        Layout {
            keys: &described.keys,
            distinct: described.distinct,
        }
    }
}

impl Naming {
    fn name(self, column: &Column) -> String {
        let mut name = match (&column.metadata, self.table_names) {
            (Metadata::Mysql(mysql), true) => format!("{}.{}", mysql.table_str(), column.name),
            _ => column.name.clone(),
        };
        match self.case {
            ColumnCase::Natural => {}
            ColumnCase::Upper => name.make_ascii_uppercase(),
            ColumnCase::Lower => name.make_ascii_lowercase(),
        }
        name
    }
}
