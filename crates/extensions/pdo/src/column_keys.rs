//! Column keys shared by every row of a result such as `SELECT id AS "postId"`.

use std::cell::OnceCell;

use phera_extension_api::{ArrayKey, PhpString};

use crate::attributes::{Attributes, ColumnCase};
use crate::drivers::Column;
use crate::metadata::Metadata;

#[derive(Default)]
pub struct ColumnKeys {
    naming: Option<Naming>,
    keys: OnceCell<Vec<Key>>,
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

    pub fn get(&self, columns: &[Column]) -> &[Key] {
        let Some(naming) = self.naming else {
            return &[];
        };
        self.keys.get_or_init(|| {
            columns
                .iter()
                .map(|column| {
                    let name = PhpString::from(naming.name(column));
                    Key {
                        array: ArrayKey::from_string(name.clone()),
                        property: ArrayKey::String(name),
                    }
                })
                .collect()
        })
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
