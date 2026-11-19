//! `match ($flag) { true => 'yes' }` still leaves false unhandled.
//! Literal and boolean cases can be subtracted without guessing about guards.

use crate::types::ValueType as T;

pub(super) struct Cases(Option<Vec<T>>);

impl Cases {
    pub(super) fn of(ty: &T, level: u8) -> Self {
        if level < 4 {
            return Self(None);
        }
        let mut pending = vec![ty];
        let mut values = Vec::new();
        while let Some(ty) = pending.pop() {
            match ty {
                T::Union(arms) => pending.extend(arms.iter()),
                T::Bool => values.extend([T::BoolLiteral(true), T::BoolLiteral(false)]),
                T::IntLiteral(_) | T::StringLiteral(_) | T::Null | T::BoolLiteral(_) => {
                    values.push(ty.clone());
                }
                _ => return Self(None),
            }
        }
        Self(Some(values))
    }

    pub(super) fn forget(&mut self) {
        self.0 = None;
    }

    pub(super) fn test(&mut self, tested: &T) -> Option<bool> {
        if !matches!(
            tested,
            T::IntLiteral(_) | T::StringLiteral(_) | T::Null | T::BoolLiteral(_)
        ) {
            self.forget();
            return None;
        }
        let cases = self.0.as_mut()?;
        let count = cases.len();
        cases.retain(|case| case != tested);
        if cases.len() == count {
            Some(false)
        } else if cases.is_empty() {
            Some(true)
        } else {
            None
        }
    }

    pub(super) fn unhandled(self) -> Option<String> {
        let values = self.0?;
        let ty = values
            .iter()
            .fold(T::Never, |combined, ty| combined.join(ty));
        (ty != T::Never).then(|| {
            let noun = if values.len() == 1 { "value" } else { "values" };
            format!("Match expression does not handle remaining {noun}: {ty}")
        })
    }
}
