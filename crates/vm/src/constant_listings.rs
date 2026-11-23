//! `get_defined_constants(true)['Core']['E_STRICT']` lists the value without a notice.

use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;

impl Vm {
    pub(crate) fn defined_constants(&self, categorize: bool) -> Value {
        let output = crate::output_buffer::CONSTANTS
            .iter()
            .map(|&(name, _)| (name, "Core"));
        let native = self.registry.constant_names().chain(output);
        let user = self.constants.user_names().map(|name| (name, "user"));
        let constants = native.chain(user).filter_map(|(name, module)| {
            self.constants
                .get(name)
                .map(|constant| (name, module, &constant.value))
        });

        let mut result = PhpArray::new();
        if categorize {
            let mut groups: Vec<(&str, PhpArray)> = Vec::new();
            for (name, module, value) in constants {
                let index = groups.iter().position(|(existing, _)| *existing == module);
                let group = match index {
                    Some(index) => &mut groups[index].1,
                    None => &mut groups.push_mut((module, PhpArray::new())).1,
                };
                group.insert(ArrayKey::str(name), value.clone());
            }
            for (module, group) in groups {
                result.insert(ArrayKey::str(module), Value::from(group));
            }
        } else {
            for (name, _, value) in constants {
                result.insert(ArrayKey::str(name), value.clone());
            }
        }
        Value::from(result)
    }
}
