//! Phera omits unused deprecated PHP items, such as `strftime('%Y')`.

use super::{Finding, Rule};

pub struct NotSupported;

impl Rule for NotSupported {
    fn identifier(&self) -> &'static str {
        "phera.notSupported"
    }

    fn level(&self) -> u8 {
        0
    }

    fn description(&self) -> &'static str {
        "Deprecated PHP items that Phera deliberately omits."
    }

    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::NotSupported(item) => Some(item.unsupported_message()),
            _ => None,
        }
    }
}
