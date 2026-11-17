//! PHP's filters for validating and sanitizing values.
//!
//! ```php
//! filter_id('int');  // FILTER_VALIDATE_INT
//! filter_list();     // ['int', 'boolean', 'float', ...]
//! ```

mod catalog;
mod constants;

use phera_extension_api::{Extension, FunctionEntry, Param, ParamType, Registry};

pub struct Filter;

impl Extension for Filter {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn register(&self, registry: &mut Registry) {
        registry.function(FunctionEntry::sync("filter_list", &[], catalog::list));
        const ID: &[Param] = &[Param::new("name", ParamType::String)];
        registry.function(FunctionEntry::sync("filter_id", ID, catalog::id));
        for &(name, value) in constants::CONSTANTS {
            registry.constant(name, value);
        }
    }
}
