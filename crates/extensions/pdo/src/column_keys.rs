//! Column keys shared by every row of a result such as `SELECT id AS "postId"`.

use std::borrow::Cow;
use std::cell::OnceCell;

use foldhash::{HashSet, HashSetExt};
use phera_extension_api::{ArrayKey, InternedStr, PhpString};

use crate::attributes::{Attributes, ColumnCase};
use crate::drivers::Column;
use crate::metadata::Metadata;

/// Column names up to this length are interned so downstream array index builds
/// and `$row['col']` lookups reuse cached hashes and pointer equality without
/// letting gigantic ad-hoc SQL aliases fill the process-wide interner.
const MAX_INTERNED_COLUMN_LEN: usize = 128;

/// Distinct-key fast-path limit: wide ORM tables (9–64 columns) build row maps
/// via `try_from_distinct_with_capacity` instead of per-column inserts.
const MAX_DISTINCT_COLUMNS: usize = 64;

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

#[derive(Clone, Copy, PartialEq, Eq)]
struct Naming {
    case: ColumnCase,
    table_names: bool,
}

pub struct Key {
    pub array: ArrayKey,
    pub property: ArrayKey,
}

impl ColumnKeys {
    pub fn describe(&mut self, previous: &[Column], columns: &[Column], attributes: &Attributes) {
        if columns.is_empty() {
            self.keys.take();
            return;
        }
        let next_naming = Naming {
            case: attributes.case,
            table_names: attributes.mysql_table_names,
        };
        if self.keys.get().is_some()
            && self.naming.is_some_and(|naming| !naming.table_names)
            && previous.len() == columns.len()
            && previous
                .iter()
                .zip(columns)
                .all(|(old, new)| old.name == new.name)
        {
            return;
        }
        self.naming.get_or_insert(next_naming);
        self.keys.take();
    }

    pub fn get(&self, columns: &[Column]) -> Layout<'_> {
        let Some(naming) = self.naming else {
            return Layout::default();
        };
        let described = self.keys.get_or_init(|| {
            let keys: Vec<_> = columns
                .iter()
                .map(|column| key_from_name(naming.name(column)))
                .collect();
            let distinct = are_keys_distinct(&keys);
            Described { keys, distinct }
        });
        Layout {
            keys: &described.keys,
            distinct: described.distinct,
        }
    }
}

fn key_from_name(name: Cow<'_, str>) -> Key {
    if name.len() <= MAX_INTERNED_COLUMN_LEN
        && let Some(interned) = InternedStr::try_intern(name.as_bytes())
    {
        return Key {
            array: interned.to_array_key(),
            property: ArrayKey::Interned(interned),
        };
    }
    let name = PhpString::from(name.into_owned());
    Key {
        array: ArrayKey::from_string(name.clone()),
        property: ArrayKey::String(name),
    }
}

fn are_keys_distinct(keys: &[Key]) -> bool {
    if keys.len() > MAX_DISTINCT_COLUMNS {
        return false;
    }
    if keys.len() <= 8 {
        return keys.iter().enumerate().all(|(index, key)| {
            !keys[..index]
                .iter()
                .any(|earlier| earlier.array == key.array)
        });
    }
    let mut seen = HashSet::with_capacity(keys.len());
    keys.iter().all(|key| seen.insert(&key.array))
}

impl Naming {
    fn name<'a>(self, column: &'a Column) -> Cow<'a, str> {
        let mut name = match (&column.metadata, self.table_names) {
            (Metadata::Mysql(mysql), true) => {
                Cow::Owned(format!("{}.{}", mysql.table_str(), column.name))
            }
            _ => Cow::Borrowed(column.name.as_str()),
        };
        match self.case {
            ColumnCase::Natural => {}
            ColumnCase::Upper => name.to_mut().make_ascii_uppercase(),
            ColumnCase::Lower => name.to_mut().make_ascii_lowercase(),
        }
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsn::DriverName;
    use crate::fetch::{FetchMode, ValueOptions, convert};
    use crate::metadata::PostgresColumn;
    use phera_extension_api::Value;
    use phera_extension_api::testing::TestContext;

    fn col(name: impl Into<String>) -> Column {
        Column {
            name: name.into(),
            metadata: Metadata::Postgres(PostgresColumn {
                oid: 0,
                table_oid: 0,
                modifier: 0,
                native_type: String::new(),
            }),
        }
    }

    fn default_attributes() -> Attributes {
        Attributes::new(DriverName::Sqlite)
    }

    #[test]
    fn twelve_distinct_columns_are_interned_and_marked_distinct() {
        let columns: Vec<_> = (0..11)
            .map(|i| col(format!("col_{i}")))
            .chain(std::iter::once(col("42")))
            .collect();
        let mut column_keys = ColumnKeys::default();
        let attributes = default_attributes();
        column_keys.describe(&[], &columns, &attributes);

        let layout = column_keys.get(&columns);
        assert!(layout.distinct);
        assert_eq!(layout.keys.len(), 12);
        for key in &layout.keys[..11] {
            assert!(matches!(key.array, ArrayKey::Interned(_)));
            assert!(matches!(key.property, ArrayKey::Interned(_)));
        }
        // Numeric column names normalize to Int for array keys while staying string for stdClass properties.
        assert_eq!(layout.keys[11].array, ArrayKey::Int(42));
        assert!(matches!(layout.keys[11].property, ArrayKey::Interned(s) if s.as_bytes() == b"42"));

        // Verify wide-row hydration through fetch::convert stays exact.
        let mut ctx = TestContext::default();
        let assoc = convert(
            &mut ctx,
            (0..12_usize).map(|i| Value::Int(i as i64)),
            layout,
            FetchMode::Assoc,
            0,
            ValueOptions::from(&attributes),
        )
        .unwrap();
        let Value::Array(arr) = assoc else {
            panic!("expected PhpArray");
        };
        assert_eq!(arr.len(), 12);
        assert_eq!(arr.get(&ArrayKey::interned(b"col_0")), Some(&Value::Int(0)));
        assert_eq!(
            arr.get(&ArrayKey::interned(b"col_10")),
            Some(&Value::Int(10))
        );
        assert_eq!(arr.get(&ArrayKey::Int(42)), Some(&Value::Int(11)));
    }

    #[test]
    fn twelve_columns_with_duplicate_disable_distinct_fast_path() {
        let mut columns: Vec<_> = (0..11).map(|i| col(format!("col_{i}"))).collect();
        columns.push(col("col_3"));
        let mut column_keys = ColumnKeys::default();
        let attributes = default_attributes();
        column_keys.describe(&[], &columns, &attributes);

        let layout = column_keys.get(&columns);
        assert!(!layout.distinct);
        assert_eq!(layout.keys.len(), 12);

        // FETCH_ASSOC keeps the last value; FETCH_NAMED groups duplicates.
        let mut ctx = TestContext::default();
        let assoc = convert(
            &mut ctx,
            (0..12_usize).map(|i| Value::Int(i as i64)),
            layout,
            FetchMode::Assoc,
            0,
            ValueOptions::from(&attributes),
        )
        .unwrap();
        let Value::Array(arr) = assoc else {
            panic!("expected PhpArray");
        };
        assert_eq!(arr.len(), 11);
        assert_eq!(
            arr.get(&ArrayKey::interned(b"col_3")),
            Some(&Value::Int(11))
        );

        let named = convert(
            &mut ctx,
            (0..12_usize).map(|i| Value::Int(i as i64)),
            layout,
            FetchMode::Named,
            0,
            ValueOptions::from(&attributes),
        )
        .unwrap();
        let Value::Array(arr) = named else {
            panic!("expected PhpArray");
        };
        let Some(Value::Array(group)) = arr.get(&ArrayKey::interned(b"col_3")) else {
            panic!("expected grouped values for duplicate col_3");
        };
        assert_eq!(group.len(), 2);
        assert_eq!(group.get(&ArrayKey::Int(0)), Some(&Value::Int(3)));
        assert_eq!(group.get(&ArrayKey::Int(1)), Some(&Value::Int(11)));
    }

    #[test]
    fn distinct_limit_and_intern_length_cap_are_enforced() {
        let attributes = default_attributes();
        let cols_64: Vec<_> = (0..64).map(|i| col(format!("c_{i}"))).collect();
        let mut keys_64 = ColumnKeys::default();
        keys_64.describe(&[], &cols_64, &attributes);
        assert!(keys_64.get(&cols_64).distinct);

        let cols_65: Vec<_> = (0..65).map(|i| col(format!("c_{i}"))).collect();
        let mut keys_65 = ColumnKeys::default();
        keys_65.describe(&[], &cols_65, &attributes);
        assert!(!keys_65.get(&cols_65).distinct);

        let exact_128 = "a".repeat(MAX_INTERNED_COLUMN_LEN);
        let over_128 = "b".repeat(MAX_INTERNED_COLUMN_LEN + 1);
        let cols = vec![col(exact_128), col(over_128)];
        let mut keys = ColumnKeys::default();
        keys.describe(&[], &cols, &attributes);
        let layout = keys.get(&cols);
        assert!(layout.distinct);
        assert!(matches!(layout.keys[0].array, ArrayKey::Interned(_)));
        assert!(matches!(layout.keys[0].property, ArrayKey::Interned(_)));
        assert!(matches!(layout.keys[1].array, ArrayKey::String(_)));
        assert!(matches!(layout.keys[1].property, ArrayKey::String(_)));
    }

    #[test]
    fn case_folding_detects_duplicates_above_eight_columns() {
        let mut columns: Vec<_> = (0..10).map(|i| col(format!("Col_{i}"))).collect();
        columns.push(col("COL_0"));
        let mut attributes = default_attributes();
        attributes.case = ColumnCase::Lower;

        let mut keys = ColumnKeys::default();
        keys.describe(&[], &columns, &attributes);
        let layout = keys.get(&columns);
        assert!(!layout.distinct);
    }

    #[test]
    fn reuses_layout_when_columns_match_and_invalidates_on_change() {
        let cols = vec![col("id"), col("randomNumber")];
        let attributes = default_attributes();
        let mut keys = ColumnKeys::default();
        keys.describe(&[], &cols, &attributes);
        let first = keys.get(&cols);
        assert_eq!(first.keys[1].array, ArrayKey::interned(b"randomNumber"));

        // Same columns reuse the cached layout.
        keys.describe(&cols, &cols, &attributes);
        let reused = keys.get(&cols);
        assert_eq!(reused.keys[1].array, ArrayKey::interned(b"randomNumber"));

        // Different column names invalidate the cache and recompute keys.
        let other = vec![col("id"), col("message")];
        keys.describe(&cols, &other, &attributes);
        let updated = keys.get(&other);
        assert_eq!(updated.keys[1].array, ArrayKey::interned(b"message"));
    }
}
