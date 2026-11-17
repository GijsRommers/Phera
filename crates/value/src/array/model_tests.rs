//! A deliberately simple Vec-of-pairs oracle, independent of storage and lookup.

use super::storage::Storage;
use super::{ArrayKey, PhpArray};
use crate::value::Value;

#[derive(Default)]
struct Model {
    pairs: Vec<(ArrayKey, Value)>,
    next: Option<i64>,
}

impl Model {
    /// Track next-free separately so append cannot validate itself through PhpArray.
    fn insert(&mut self, key: ArrayKey, value: Value) {
        if let ArrayKey::Int(index) = key
            && self.next.is_none_or(|next| index >= next)
        {
            self.next = Some(index.saturating_add(1));
        }
        match self.pairs.iter_mut().find(|(existing, _)| *existing == key) {
            Some(pair) => pair.1 = value,
            None => self.pairs.push((key, value)),
        }
    }

    fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        self.pairs
            .iter()
            .position(|(existing, _)| existing == key)
            .map(|position| self.pairs.remove(position).1)
    }

    /// Pop alone may roll the remembered next-free key backwards.
    fn pop(&mut self) -> Option<Value> {
        let (key, value) = self.pairs.pop()?;
        if let (ArrayKey::Int(index), Some(next)) = (key, self.next)
            && next == index.saturating_add(1)
        {
            self.next = Some(index);
        }
        Some(value)
    }

    /// Shift renumbers integers only, leaving string keys in place.
    fn shift(&mut self) -> Option<Value> {
        if self.pairs.is_empty() {
            return None;
        }
        let value = self.pairs.remove(0).1;
        let mut next = 0;
        for (key, _) in &mut self.pairs {
            if matches!(key, ArrayKey::Int(_)) {
                *key = ArrayKey::Int(next);
                next += 1;
            }
        }
        self.next = Some(next);
        Some(value)
    }
}

/// Exercise actual hole positions as well as normalized and extreme keys after every step.
#[test]
fn every_lookup_agrees_with_a_plain_list_of_pairs() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut random = |bound: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % bound
    };
    let mut array = PhpArray::new();
    let mut model = Model::default();
    let mut transitions = [[0; 2]; 2];
    let mut list_holes = 0;
    let mut hashed_visits = 0;
    let mut demotions = 0;
    for step in 0..1_000_000 {
        let before = usize::from(matches!(array.storage, Storage::Map(_)));
        let key = match random(8) {
            0 => ArrayKey::str(["a", "b", "c", "d", "e", "f", "g", "h", "i"][random(9) as usize]),
            1 => ArrayKey::Int(i64::MIN),
            2 => ArrayKey::Int(i64::MAX),
            3 => ArrayKey::Int(1_i64 << 40),
            4 => ArrayKey::from_value(&Value::from(
                ["0", "5", "-5", "05", "-0", "9223372036854775807"][random(6) as usize],
            ))
            .unwrap(),
            _ => ArrayKey::Int(random(24) as i64 - 4),
        };
        let operation = random(16);
        match operation {
            0..=3 => {
                array.insert(key.clone(), Value::Int(step));
                model.insert(key, Value::Int(step));
            }
            4..=6 => {
                let result = array.push(Value::Int(step));
                if model.next == Some(i64::MAX) {
                    assert!(result.is_err());
                } else {
                    let expected = ArrayKey::Int(model.next.unwrap_or(0));
                    assert_eq!(result.unwrap(), expected);
                    model.insert(expected, Value::Int(step));
                }
            }
            7 => assert_eq!(array.remove(&key), model.remove(&key)),
            8..=9 => {
                // Delete at a live iterator position. Removal must leave all later positions.
                let positions: Vec<_> = (0..array.storage.len())
                    .filter(|&position| array.entry_at(position).is_some())
                    .collect();
                if !positions.is_empty() {
                    let position = positions[random(positions.len() as u64) as usize];
                    let key = array.entry_at(position).unwrap().0;
                    let next = array.next_position(position);
                    let following = next.map(|index| (index, array.entry_at(index).unwrap().0));
                    assert_eq!(array.remove(&key), model.remove(&key));
                    assert!(array.entry_at(position).is_none());
                    assert_eq!(array.next_position(position), next);
                    if let Some((index, key)) = following {
                        assert_eq!(array.entry_at(index).unwrap().0, key);
                    }
                }
            }
            10 => assert_eq!(array.pop(), model.pop()),
            11..=12 => assert_eq!(array.shift(), model.shift()),
            13 => {
                if let Some(value) = array.get_mut(&key) {
                    *value = Value::Int(-step);
                }
                if let Some(pair) = model.pairs.iter_mut().find(|(stored, _)| *stored == key) {
                    pair.1 = Value::Int(-step);
                }
            }
            14 => {
                for (_, value) in array.iter_mut() {
                    *value = Value::Int(step);
                }
                for (_, value) in &mut model.pairs {
                    *value = Value::Int(step);
                }
            }
            _ => {
                // Fresh epochs repeatedly drive lists, holes, promotion, and demotion.
                array = PhpArray::new();
                model = Model::default();
            }
        }
        let after = usize::from(matches!(array.storage, Storage::Map(_)));
        transitions[before][after] += 1;
        if before == 1 && after == 0 && operation != 15 {
            demotions += 1;
        }
        if matches!(&array.storage, Storage::Map(map) if map.index.is_some()) {
            hashed_visits += 1;
        }
        if after == 0 && array.dead > 0 {
            list_holes += 1;
        }
        assert_eq!(array.len(), model.pairs.len(), "step {step}");
        assert_eq!(array.next_free_index(), model.next, "step {step}");
        assert_eq!(
            array
                .iter()
                .map(|(key, value)| (key, value.clone()))
                .collect::<Vec<_>>(),
            model.pairs,
            "step {step}"
        );
        for (key, value) in &model.pairs {
            assert_eq!(array.get(key), Some(value), "step {step}, key {key:?}");
        }
        assert_eq!(array.get(&ArrayKey::str("absent")), None);
    }
    assert!(transitions.into_iter().flatten().all(|count| count > 100));
    assert!(list_holes > 100);
    assert!(hashed_visits > 100);
    assert!(demotions > 100);
}
