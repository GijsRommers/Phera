//! PHP's filters for validating and sanitizing values.
//!
//! ```php
//! filter_id('int');  // FILTER_VALIDATE_INT
//! filter_list();     // ['int', 'boolean', 'float', ...]
//! ```

mod apply;
mod catalog;
mod constants;
mod domain;
mod email;
mod exceptions;
mod functions;
mod html;
mod html_table;
mod input;
mod ip;
mod numbers;
mod options;
mod regexp;
mod sanitize;
mod stringify;
mod tree;
mod url;

use phera_extension_api::{Extension, FunctionEntry, Param, ParamType, Registry};

pub use input::{capture_environment, capture_http_inputs, capture_server};

pub struct Filter;

impl Extension for Filter {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn register(&self, registry: &mut Registry) {
        registry.function(FunctionEntry::sync("filter_list", &[], catalog::list));
        const ID: &[Param] = &[Param::new("name", ParamType::Mixed)];
        registry.function(FunctionEntry::asynchronous("filter_id", ID, catalog::id));
        functions::register(registry);
        input::register(registry);
        exceptions::register(registry);
        for &(name, value) in constants::CONSTANTS {
            registry.constant(name, value);
        }
    }
}
