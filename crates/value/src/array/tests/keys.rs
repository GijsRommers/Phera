//! `$a[$key]` normalises the key: `$a["5"]` and `$a[5]` are the same entry.

use super::{k, ks};
use crate::array::{ArrayKey, PhpArray};
use crate::error::CollectDiagnostics;
use crate::object::{ClassHandle, Object};
use crate::value::Value;
use std::rc::Rc;

/// Canonical decimal strings become integer keys; other strings stay strings,
/// and arrays and objects cannot be keys at all.
#[test]
fn key_normalisation() {
    assert_eq!(ks("5"), k(5));
    assert_eq!(ks("-5"), k(-5));
    assert_eq!(ks("0"), k(0));
    assert_eq!(ks("9223372036854775807"), k(i64::MAX));
    assert_eq!(ks("-9223372036854775808"), k(i64::MIN));
    for s in [
        "05",
        "5.0",
        " 5",
        "+5",
        "-0",
        " 5 ",
        "08",
        "1e3",
        "9223372036854775808",
        "-",
        "",
    ] {
        assert_eq!(ks(s), ArrayKey::str(s), "key {s:?} must stay a string");
    }
    assert_eq!(ArrayKey::from_value(&Value::Int(-7)).unwrap(), k(-7));
    assert_eq!(ArrayKey::from_value(&Value::Bool(true)).unwrap(), k(1));
    assert_eq!(ArrayKey::from_value(&Value::Bool(false)).unwrap(), k(0));
    assert_eq!(ArrayKey::from_value(&Value::Null).unwrap(), ks(""));
    assert_eq!(ArrayKey::from_value(&Value::Float(5.0)).unwrap(), k(5));
    assert_eq!(ArrayKey::from_value(&Value::Float(-5.9)).unwrap(), k(-5));
    let arr = Value::Array(Rc::new(PhpArray::new()));
    assert_eq!(
        ArrayKey::from_value(&arr).unwrap_err().message,
        "Cannot access offset of type array on array"
    );
    let class: Rc<dyn ClassHandle> = Rc::new(DummyClass {
        name: "stdClass".into(),
    });
    let obj = Value::Object(Rc::new(Object::new(1, class)));
    assert_eq!(
        ArrayKey::from_value(&obj).unwrap_err().message,
        "Cannot access offset of type stdClass on array"
    );
}

struct DummyClass {
    name: String,
}
impl ClassHandle for DummyClass {
    fn name(&self) -> &str {
        &self.name
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `$a[null]` is `$a[""]`, with a deprecation since PHP 8.5.
#[test]
fn null_key_deprecation() {
    let mut d = CollectDiagnostics::default();
    ArrayKey::from_value_diag(&Value::Null, &mut d).unwrap();
    assert_eq!(
        d.messages(),
        vec!["Using null as an array offset is deprecated, use an empty string instead"]
    );
}
