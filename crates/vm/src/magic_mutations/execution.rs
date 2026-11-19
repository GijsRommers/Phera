//! A lexical write retains its mutation while an overloaded offset or getter
//! runs. The retained cursor continues without re-reading a PHP expression.

use phera_extension_api::NativeError;
use phera_value::Value;

use super::{MutationPath, MutationStep, OverloadedMutation, PropertyMutation};
use crate::access_mode::{Creation, Missing, Mutation};
use crate::inline_stack::InlineStack;
use crate::{AccessPath, Vm};

impl Vm {
    pub(crate) fn start_magic_mutation(
        &mut self,
        place: AccessPath,
        operation: PropertyMutation,
    ) -> Result<MutationPath, NativeError> {
        let creation = match operation {
            PropertyMutation::Write(..) => Creation::Create,
            PropertyMutation::Unset => Creation::Existing,
        };
        let register = if place.steps()?.next().is_some() {
            self.base_register(&place, creation, Missing::Silent)?
        } else {
            None
        };
        let cursor = register.map_or(Value::Null, |register| {
            self.read_root(register, Missing::Silent)
        });
        Ok(MutationPath {
            place,
            operation,
            register,
            cursor,
            detached: None,
            resolved: InlineStack::new(),
            next: 0,
            retry: None,
        })
    }

    pub(crate) async fn advance_magic_mutation(
        &mut self,
        path: &mut MutationPath,
    ) -> Result<MutationStep, NativeError> {
        let plan = self.plan_magic_mutation(path).await?;
        let place = &path.place;
        let operation = std::mem::replace(&mut path.operation, PropertyMutation::Unset);
        match plan {
            OverloadedMutation::Lazy(object) => {
                path.operation = operation;
                return Ok(MutationStep::Lazy(object));
            }
            OverloadedMutation::Hook(hook) => {
                path.operation = operation;
                return Ok(MutationStep::Hook(hook));
            }
            OverloadedMutation::Ordinary(register, keys) => match operation {
                PropertyMutation::Write(value, binding) => {
                    let mut value = value.into_value(self);
                    let current = self.read_root(register, Missing::Silent);
                    if !self.validate_property_mutation(
                        current,
                        &keys,
                        &mut value,
                        binding.into(),
                    )? {
                        self.write_resolved_place(place, register, *keys, value, binding)?;
                    }
                    Ok(None)
                }
                PropertyMutation::Unset => {
                    self.unset_resolved_place(register, &keys).map(|()| None)
                }
            },
            OverloadedMutation::Unresolved => match operation {
                PropertyMutation::Write(value, binding) => {
                    let value = value.into_value(self);
                    self.write_place(place, value, binding).map(|()| None)
                }
                PropertyMutation::Unset => self.unset_place(place).map(|()| None),
            },
            OverloadedMutation::Handled => {
                if let PropertyMutation::Write(mut value, _) = operation {
                    value.read(self);
                }
                Ok(None)
            }
            OverloadedMutation::Converted(value) => Ok(Some(value)),
            OverloadedMutation::Dimension(path) => {
                self.mutate_property_dimension(path, operation).await?;
                Ok(None)
            }
            OverloadedMutation::Detached(root, keys) => {
                let (value, mutation) = match operation {
                    PropertyMutation::Write(value, binding) => {
                        (value.into_value(self), binding.into())
                    }
                    PropertyMutation::Unset => (Value::Null, Mutation::Unset),
                };
                self.write_detached_place(root, &keys, value, mutation)
                    .await
            }
        }
        .map(MutationStep::Complete)
    }
}
