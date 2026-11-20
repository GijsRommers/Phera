//! Composer's dependency-weight ordering for eager files.
//!
//! If `app` needs `framework` which needs `polyfill`, polyfill files load first.

use crate::Package;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn packages(packages: &[Package]) -> Vec<&Package> {
    let mut users: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for package in packages {
        if let Some(requires) = package.field("require").as_object() {
            for name in requires.keys() {
                users.entry(name).or_default().push(&package.name);
            }
        }
    }
    let mut weights = BTreeMap::new();
    let mut computing = BTreeSet::new();
    for package in packages {
        weight(&package.name, &users, &mut weights, &mut computing);
    }
    let mut ordered: Vec<_> = packages.iter().collect();
    ordered.sort_by(|left, right| {
        weights
            .get(left.name.as_str())
            .cmp(&weights.get(right.name.as_str()))
            .then_with(|| natural_compare(&left.name, &right.name))
    });
    ordered
}

fn natural_compare(left: &str, right: &str) -> std::cmp::Ordering {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    let (mut a, mut b) = (0, 0);
    while a < left.len() && b < right.len() {
        if left[a].is_ascii_digit() && right[b].is_ascii_digit() {
            let end_a = a + left[a..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            let end_b = b + right[b..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            let digits = left[a..end_a].cmp(&right[b..end_b]);
            let order = if left[a] == b'0' || right[b] == b'0' {
                digits
            } else {
                (end_a - a).cmp(&(end_b - b)).then(digits)
            };
            if !order.is_eq() {
                return order;
            }
            (a, b) = (end_a, end_b);
        } else {
            let order = left[a]
                .to_ascii_lowercase()
                .cmp(&right[b].to_ascii_lowercase());
            if !order.is_eq() {
                return order;
            }
            a += 1;
            b += 1;
        }
    }
    left.len().cmp(&right.len())
}

fn weight<'a>(
    name: &'a str,
    users: &BTreeMap<&str, Vec<&'a str>>,
    weights: &mut BTreeMap<&'a str, i64>,
    computing: &mut BTreeSet<&'a str>,
) -> i64 {
    if let Some(weight) = weights.get(name) {
        return *weight;
    }
    if !computing.insert(name) {
        return 0;
    }
    let mut value = 0;
    if let Some(dependents) = users.get(name) {
        for dependent in dependents {
            value -= 1 - weight(dependent, users, weights, computing);
        }
    }
    computing.remove(name);
    weights.insert(name, value);
    value
}

#[cfg(test)]
mod tests {
    #[test]
    fn composer_natural_name_order_breaks_dependency_weight_ties() {
        // Observed from PHP 8.5.12 strnatcasecmp, used by Composer PackageSorter.
        assert!(super::natural_compare("acme/lib10", "acme/lib2").is_gt());
        assert!(super::natural_compare("acme/lib02", "acme/lib2").is_lt());
        assert!(super::natural_compare("acme/lib002", "acme/lib02").is_lt());
    }
}
