//! Adversarial callbacks must preserve positions and finish within a finite budget.

use std::cmp::Ordering;

use phera_ext_standard::php_sort_order;

#[test]
fn inconsistent_comparisons_preserve_every_position() {
    let mut seed = 0x4d59_5df4_d0f3_3173_u64;
    let mut lengths: Vec<usize> = (0..=2_000).collect();
    for _ in 0..2_000 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        lengths.push((seed >> 32) as usize % 2_001);
    }
    // Cover every length, including both endpoints, plus 2,000 seeded trials.
    for length in lengths {
        let mut comparisons = 0;
        let result = php_sort_order(length, |_, _| {
            comparisons += 1;
            // A callback budget turns a non-progressing scan into a test failure.
            assert!(comparisons <= 32 * (length + 1) * (length + 1));
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            Ok(match seed >> 32 & 3 {
                0 => Ordering::Less,
                1 => Ordering::Equal,
                _ => Ordering::Greater,
            })
        });
        let Ok(mut positions) = result else {
            panic!("infallible comparator returned an error");
        };
        positions.sort_unstable();
        assert_eq!(positions, (0..length).collect::<Vec<_>>());
    }
}

#[test]
fn callback_sorts_finish_with_constant_comparisons_on_large_inputs() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use phera_ext_standard::Standard;
    use phera_extension_api::testing::TestContext;
    use phera_extension_api::{
        Context, Diagnostics, Extension, LocalFuture, NativeFn, NativeResult, PhpArray, Registry,
        RequestState, Value, prepare_arguments,
    };

    struct ConstantComparator {
        base: TestContext,
        result: i64,
        calls: usize,
    }
    impl Context for ConstantComparator {
        fn write(&mut self, bytes: &[u8]) {
            self.base.write(bytes);
        }
        fn diagnostics(&mut self) -> &mut dyn Diagnostics {
            self.base.diagnostics()
        }
        fn state(&mut self) -> &mut RequestState {
            self.base.state()
        }
        fn strict_types(&self) -> bool {
            false
        }
        fn call<'a>(&'a mut self, _: &'a Value, _: Vec<Value>) -> LocalFuture<'a, NativeResult> {
            self.calls += 1;
            assert!(self.calls < 10_000_000, "comparator budget exceeded");
            Box::pin(async { Ok(Value::Int(self.result)) })
        }
    }
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    for name in ["usort", "uasort", "uksort"] {
        for result in [-1, 0, 1] {
            // A callback that always answers "greater" makes zend_sort quadratic,
            // in PHP as well (100,000 elements need 2.5 billion comparisons and
            // minutes). A smaller input checks the same code path in a second.
            let length: i64 = if result == 1 { 3_000 } else { 100_000 };
            let mut array = PhpArray::new();
            for number in (0..length).rev() {
                array.push(Value::Int(number)).expect("push");
            }
            let target = Rc::new(RefCell::new(Value::from(array)));
            let mut arguments = vec![Value::Reference(target.clone()), Value::from("constant")];
            let entry = registry.get_function(name).expect("callback sort");
            let mut context = ConstantComparator {
                base: TestContext::default(),
                result,
                calls: 0,
            };
            prepare_arguments(entry, &mut arguments, &mut context).expect("arguments");
            let NativeFn::Async(handler) = entry.handler else {
                panic!("async sort required");
            };
            let mut future = handler(&mut context, arguments);
            let mut task = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(matches!(
                future.as_mut().poll(&mut task),
                std::task::Poll::Ready(Ok(Value::Bool(true)))
            ));
            drop(future);
            println!("{name}, constant {result}: {} comparisons", context.calls);
            let value = target.borrow();
            let Value::Array(array) = &*value else {
                panic!("lost array");
            };
            assert_eq!(array.len(), length as usize);
            let mut values = array
                .values()
                .map(|value| match &*value.deref() {
                    Value::Int(number) => *number,
                    _ => panic!("changed value"),
                })
                .collect::<Vec<_>>();
            values.sort_unstable();
            assert_eq!(values, (0..length).collect::<Vec<_>>());
        }
    }
}
