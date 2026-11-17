//! Request state and deliberately checked UTF handling do not require a VM.

use phera_ext_pcre::Pcre;
use phera_extension_api::testing::{TestContext, call};
use phera_extension_api::{Extension, Registry, Value};

#[test]
fn errors_are_request_local_and_quote_preserves_them() {
    let mut registry = Registry::new();
    Pcre.register(&mut registry);
    let mut first = TestContext::default();
    let mut second = TestContext::default();
    call(
        &registry,
        &mut first,
        "preg_match",
        vec![Value::from("/./u"), Value::from(vec![0xff])],
    )
    .unwrap();
    call(&registry, &mut first, "preg_quote", vec![Value::from(".")]).unwrap();
    assert_eq!(
        call(&registry, &mut first, "preg_last_error", vec![]).unwrap(),
        Value::Int(4)
    );
    assert_eq!(
        call(&registry, &mut second, "preg_last_error", vec![]).unwrap(),
        Value::Int(0)
    );
    call(
        &registry,
        &mut first,
        "preg_match",
        vec![Value::from("/a/"), Value::from("b")],
    )
    .unwrap();
    assert_eq!(
        call(&registry, &mut first, "preg_last_error", vec![]).unwrap(),
        Value::Int(0)
    );
}

#[test]
fn inline_utf_uses_checked_matching_unlike_php_without_u() {
    let mut registry = Registry::new();
    Pcre.register(&mut registry);
    let mut context = TestContext::default();
    // Do not send malformed input to PHP's unchecked (*UTF) path: PCRE2 documents
    // undefined behavior, and the initial oracle run returned out-of-subject bytes.
    // This explicitly records a compatibility limitation, not a parity assertion.
    let result = call(
        &registry,
        &mut context,
        "preg_match",
        vec![Value::from("/(*UTF)./"), Value::from(vec![0xff])],
    )
    .unwrap();
    assert_eq!(result, Value::Bool(false));
    assert_eq!(
        call(&registry, &mut context, "preg_last_error", vec![]).unwrap(),
        Value::Int(4)
    );
}
