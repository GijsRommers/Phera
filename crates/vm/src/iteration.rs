//! Foreach snapshots and live reference traversal.
use crate::{Place, Vm, frame};
use phera_bytecode::{IterId, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};
impl Vm {
    pub(crate) fn iter_start(
        &mut self,
        iter: IterId,
        src: Reg,
        place: Option<Place>,
    ) -> Result<(), NativeError> {
        let v = self.get(src);
        let snapshot = if let Value::Array(a) = v {
            Some(a)
        } else {
            self.warn(format!(
                "foreach() argument must be of type array|object, {} given",
                v.type_name()
            ));
            None
        };
        if let Some(f) = self.frames.last_mut() {
            f.iterators.insert(
                iter.0,
                frame::IteratorState {
                    snapshot: if place.is_none() { snapshot } else { None },
                    source: place,
                    position: None,
                    started: false,
                },
            );
        }

        Ok(())
    }
    pub(crate) fn iter_start_reference(
        &mut self,
        iter: IterId,
        place: Place,
    ) -> Result<(), NativeError> {
        let value = self.read_place(&place, false)?;
        if !value.is_array() {
            self.warn(format!(
                "foreach() argument must be of type array|object, {} given",
                value.type_name()
            ));
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.iterators.insert(
                iter.0,
                frame::IteratorState {
                    snapshot: None,
                    source: Some(place),
                    position: None,
                    started: false,
                },
            );
        }
        Ok(())
    }
    pub(crate) fn iter_next(
        &mut self,
        iter: IterId,
        key: Option<Reg>,
        value: Reg,
        end: Target,
    ) -> Result<(), NativeError> {
        let mut state = self
            .frames
            .last_mut()
            .and_then(|f| f.iterators.remove(&iter.0))
            .ok_or_else(|| NativeError::error("Phera internal error: missing iterator"))?;
        let array = if let Some(p) = &state.source {
            match self.read_place(p, true)? {
                Value::Array(a) => Some(a),
                _ => None,
            }
        } else {
            state.snapshot.clone()
        };
        let position = array.as_ref().and_then(|a| {
            if state.started {
                state.position.and_then(|p| a.next_position(p))
            } else {
                a.first_position()
            }
        });
        state.started = true;
        state.position = position;
        let entry = array
            .as_ref()
            .and_then(|a| position.and_then(|p| a.entry_at(p)))
            .map(|(k, v)| (k.clone(), v.deref().clone()));
        drop(array);
        if let Some((k, mut v)) = entry {
            let key_register = key.unwrap_or(value);
            self.set(
                key_register,
                match k {
                    ArrayKey::Int(i) => Value::Int(i),
                    ArrayKey::String(s) => Value::String(s),
                },
            );
            if let Some(p) = &state.source {
                let mut p = p.clone();
                p.path.push(Some(key_register));
                v = self.reference(&p)?;
            }
            self.set(value, v);
        } else {
            self.jump(end)?
        }
        if let Some(f) = self.frames.last_mut() {
            f.iterators.insert(iter.0, state);
        }

        Ok(())
    }
}
