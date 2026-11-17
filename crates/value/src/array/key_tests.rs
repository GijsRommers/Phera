use super::{ArrayKey, PhpArray};
use crate::error::CollectDiagnostics;
use crate::object::{ClassHandle, Object};
use crate::string::PhpString;
use crate::value::Value;
use std::rc::Rc;

fn k(i: i64) -> ArrayKey {
    ArrayKey::Int(i)
}
fn ks(s: &str) -> ArrayKey {
    // Normalise exactly like `$a[$s] = ...` does in PHP.
    ArrayKey::from_value(&Value::String(PhpString::from(s))).unwrap()
}

#[test]
fn key_normalisation() {
    let t = |s: &str| ArrayKey::from_value(&Value::String(PhpString::from(s))).unwrap();
    assert_eq!(t("5"), k(5));
    assert_eq!(t("-5"), k(-5));
    assert_eq!(t("0"), k(0));
    assert_eq!(t("9223372036854775807"), k(i64::MAX));
    assert_eq!(t("-9223372036854775808"), k(i64::MIN));
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
        assert_eq!(t(s), ks(s), "key {s:?} must stay a string");
    }
    // int/bool/null/float
    assert_eq!(ArrayKey::from_value(&Value::Int(-7)).unwrap(), k(-7));
    assert_eq!(ArrayKey::from_value(&Value::Bool(true)).unwrap(), k(1));
    assert_eq!(ArrayKey::from_value(&Value::Bool(false)).unwrap(), k(0));
    assert_eq!(ArrayKey::from_value(&Value::Null).unwrap(), ks(""));
    assert_eq!(ArrayKey::from_value(&Value::Float(5.0)).unwrap(), k(5));
    assert_eq!(ArrayKey::from_value(&Value::Float(-5.9)).unwrap(), k(-5));
    // arrays and objects are TypeErrors
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

#[test]
fn null_key_deprecation() {
    let mut d = CollectDiagnostics::default();
    ArrayKey::from_value_diag(&Value::Null, &mut d).unwrap();
    assert_eq!(
        d.messages(),
        vec!["Using null as an array offset is deprecated, use an empty string instead"]
    );
}
