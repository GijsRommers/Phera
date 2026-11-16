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
