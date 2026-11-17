//! `DateTimeZone::getOffset($date)` reads the date's instant.

use phera_extension_api::{NativeError, Object};

use super::state;

#[derive(Clone)]
pub struct Instant {
    pub timestamp: i64,
}

impl Instant {
    pub fn read(object: &Object) -> Result<Self, NativeError> {
        let base = if object.class.is_instance_of("DateTimeImmutable") {
            "DateTimeImmutable"
        } else {
            "DateTime"
        };
        state(object, base)
    }
}
