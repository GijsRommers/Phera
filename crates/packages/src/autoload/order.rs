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
            .then_with(|| left.name.cmp(&right.name))
    });
    ordered
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
