//! Arguments of the wrong type where PHP's own message names a class:
//! `Random\Engine` for the Randomizer's engine, `Random\IntervalBoundary`
//! for `getFloat()`'s boundary. PHP names `true` and `false`, not `bool`.

use phera_extension_api::testing::{TestContext, call_method, instantiate, oracle};
use phera_extension_api::{PhpArray, Value};

use crate::support::{Comparison, engine, randomizer, registry};

#[test]
fn values_that_are_not_engines_or_boundaries() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    let engine = engine(&registry, &mut context, "Mt19937", vec![Value::Int(1)]);
    let seeded = randomizer(&registry, &mut context, &engine);
    comparison.setup("$r = new Random\\Randomizer(new Random\\Engine\\Mt19937(1));");
    let values = [
        Value::Bool(true),
        Value::Bool(false),
        Value::Int(1),
        Value::Float(1.5),
        Value::from("x"),
        Value::from(PhpArray::new()),
    ];
    for value in values {
        let literal = oracle::php_literal(&value);

        let randomizer = instantiate(&registry, "Random\\Randomizer").unwrap();
        let construct = vec![value.clone()];
        let result = call_method(
            &registry,
            &mut context,
            &randomizer,
            "__construct",
            construct,
        );
        // A constructor returns nothing, so compare null on success.
        comparison.record(
            &format!("(new Random\\Randomizer({literal})) ? null : null"),
            &result,
            &mut context,
        );

        let arguments = vec![Value::Float(0.0), Value::Float(1.0), value];
        let result = call_method(&registry, &mut context, &seeded, "getFloat", arguments);
        comparison.record(
            &format!("$r->getFloat(0.0, 1.0, {literal})"),
            &result,
            &mut context,
        );
    }
    comparison.compare(
        "random-argument-errors",
        "engines and boundaries of the wrong type",
    );
}
