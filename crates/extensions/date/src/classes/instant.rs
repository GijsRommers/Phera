//! `$date->setTimezone($zone)` preserves the instant and changes its civil representation.

use phera_extension_api::{Context, NativeResult, Object, Value};

use super::{make_with_properties, state, store, zone_kind::ZoneKind};
use crate::{calendar::Civil, format::DateFormat, parse::state::State};

pub const MUTABLE: &str = "DateTime";
pub const IMMUTABLE: &str = "DateTimeImmutable";
pub const INTERFACE: &str = "DateTimeInterface";

#[derive(Clone, Debug)]
pub struct Instant {
    pub timestamp: i64,
    pub micros: i64,
    pub zone: ZoneKind,
    // setDate/setISODate retain the requested wall time even in an offset gap.
    pub civil_override: Option<Civil>,
}

impl Instant {
    pub fn new(timestamp: i64, micros: i64, zone: ZoneKind) -> Self {
        Self {
            timestamp,
            micros,
            zone,
            civil_override: None,
        }
    }

    pub fn read(object: &Object) -> Result<Self, phera_extension_api::NativeError> {
        state(object, Self::base_class(object))
    }

    pub fn base_class(object: &Object) -> &'static str {
        if object.class.is_instance_of(IMMUTABLE) {
            IMMUTABLE
        } else {
            MUTABLE
        }
    }

    pub fn format(&self) -> DateFormat {
        let mut format = self.zone.format(self.timestamp, self.micros);
        if let Some(civil) = self.civil_override {
            format.civil = civil;
        }
        format
    }

    pub fn wall(&mut self, civil: Civil, micros: i64, daylight: Option<bool>, keep_wall: bool) {
        let civil = crate::normalize::civil(Civil {
            second: civil.second.wrapping_add(micros.div_euclid(1_000_000)),
            ..civil
        });
        self.timestamp = self.zone.resolve(civil, daylight);
        self.micros = micros.rem_euclid(1_000_000);
        self.civil_override = keep_wall.then_some(civil);
    }

    pub fn apply(&mut self, parsed: &State, base: Civil, daylight: Option<bool>) {
        let civil = crate::parse::apply::wall(parsed, base);
        self.timestamp = self.zone.resolve(civil, daylight);
        self.micros = parsed
            .parsed
            .microsecond
            .unwrap_or(0)
            .wrapping_add(parsed.delta[6])
            .rem_euclid(1_000_000);
        self.civil_override = None;
    }

    pub fn changed(self, context: &mut dyn Context, object: &std::rc::Rc<Object>) -> NativeResult {
        if Self::base_class(object) == IMMUTABLE {
            make_with_properties(context, object.class.name(), self, object.to_array())
        } else {
            store(object, self);
            Ok(Value::Object(object.clone()))
        }
    }
}
