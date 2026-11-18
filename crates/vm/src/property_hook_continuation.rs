//! A getter in `$object->{$name}->child` leaves its cursor on a PHP frame.
//! Its return resumes the walk from that cursor, with no nested VM call.

use std::rc::Rc;

use phera_bytecode::{BinaryOp, FetchMode, IncDec, Reg};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::calls::call_execution::CallEntry;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::code_table::FunctionIndex;
use crate::hooked_property_iteration::IterationGetter;
use crate::magic_mutations::{
    AssignmentLocal, MutationHook, MutationPath, MutationStep, PropertyMutation,
};
use crate::magic_places::{PathRead, PathReadStep};
use crate::magic_references::{ReferenceGetter, ReferencePath, ReferenceStep};
use crate::{AccessPath, Vm};

pub(crate) enum ReadCompletion {
    Fetch(Reg),
    Argument(usize),
    AssignOp(BinaryOp, Reg, Reg),
    IncDec(IncDec, Reg),
}

pub(crate) enum ReferenceCompletion {
    Fetch(Reg),
    Argument(usize),
}

impl ReferenceCompletion {
    fn finish(self, vm: &mut Vm, value: Value) -> Result<(), NativeError> {
        match self {
            Self::Fetch(dst) => {
                vm.set(dst, value);
                Ok(())
            }
            Self::Argument(index) => vm.store_sent_value(index, value),
        }
    }
}

pub(crate) enum WriteCompletion {
    Discard,
    Cast(Reg, Option<Value>),
    Local(Reg),
    Update(Reg, Value),
}

impl WriteCompletion {
    pub(crate) fn finish(self, vm: &mut Vm, converted: Option<Value>) {
        match self {
            Self::Discard => {}
            Self::Cast(src, fallback) => {
                if let Some(value) = converted.or(fallback) {
                    vm.set_cast_assignment_result(src, value);
                }
            }
            Self::Local(dst) => {
                if let Some(value) = converted {
                    vm.set(dst, value);
                }
            }
            Self::Update(dst, value) => vm.set(dst, value),
        }
    }
}

pub(crate) enum HookContinuation {
    Read {
        path: PathRead,
        completion: ReadCompletion,
        indirect: Option<(Rc<Object>, String)>,
    },
    Mutation {
        path: MutationPath,
        completion: WriteCompletion,
        getter: MutationGetter,
    },
    Written {
        local: Option<AssignmentLocal>,
        completion: WriteCompletion,
    },
    Reference {
        path: ReferencePath,
        completion: ReferenceCompletion,
        getter: ReferenceGetter,
    },
    Iteration(IterationGetter),
}

pub(crate) struct MutationGetter {
    object: Rc<Object>,
    name: String,
    last: bool,
    indirect: bool,
}

pub(crate) struct ReturnedHook {
    pub(crate) continuation: Box<HookContinuation>,
    pub(crate) value: Value,
}

impl Vm {
    pub(crate) async fn read_property_path(
        &mut self,
        place: AccessPath,
        mode: FetchMode,
        completion: ReadCompletion,
    ) -> Result<(), NativeError> {
        let update = matches!(
            completion,
            ReadCompletion::AssignOp(..) | ReadCompletion::IncDec(..)
        );
        let path = self.start_read_path(place, mode, update)?;
        self.continue_property_path(path, completion).await
    }

    pub(crate) async fn resume_property_hook(
        &mut self,
        returned: ReturnedHook,
    ) -> Result<(), NativeError> {
        match *returned.continuation {
            HookContinuation::Read {
                mut path,
                completion,
                indirect,
            } => {
                self.resume_path_read(&mut path, returned.value, indirect)?;
                self.continue_property_path(path, completion).await
            }
            HookContinuation::Mutation {
                mut path,
                completion,
                getter,
            } => {
                self.resume_magic_mutation(
                    &mut path,
                    returned.value,
                    &getter.object,
                    &getter.name,
                    getter.last,
                    getter.indirect,
                )?;
                self.continue_property_mutation(path, completion).await
            }
            HookContinuation::Written { local, completion } => {
                if let Some(local) = local {
                    local.refresh_result(self);
                }
                completion.finish(self, None);
                Ok(())
            }
            HookContinuation::Reference {
                mut path,
                completion,
                getter,
            } => {
                if let Some(value) = path.resume(getter, returned.value)? {
                    completion.finish(self, value)
                } else {
                    self.continue_property_reference(path, completion).await
                }
            }
            HookContinuation::Iteration(getter) => {
                self.resume_hooked_iteration(getter, returned.value)
            }
        }
    }

    async fn continue_property_path(
        &mut self,
        mut path: PathRead,
        completion: ReadCompletion,
    ) -> Result<(), NativeError> {
        match self.advance_path_read(&mut path).await? {
            PathReadStep::Complete(result) => match completion {
                ReadCompletion::Fetch(destination) => {
                    self.set(destination, result.value);
                    Ok(())
                }
                ReadCompletion::Argument(index) => self.store_sent_value(index, result.value),
                ReadCompletion::AssignOp(op, dst, src) => {
                    self.finish_magic_assign_op(&path.place, result, op, dst, src)
                        .await
                }
                ReadCompletion::IncDec(mode, dst) => {
                    self.finish_magic_inc_dec(&path.place, result, mode, dst)
                        .await
                }
            },
            PathReadStep::Getter {
                function,
                object,
                indirect,
            } => {
                let indirect = indirect.map(|name| (object.clone(), name));
                self.suspend_for_hook(
                    function,
                    object,
                    None,
                    HookContinuation::Read {
                        path,
                        completion,
                        indirect,
                    },
                )
                .await
            }
        }
    }

    pub(crate) async fn mutate_property_path(
        &mut self,
        place: AccessPath,
        operation: PropertyMutation,
        completion: WriteCompletion,
    ) -> Result<(), NativeError> {
        let path = self.start_magic_mutation(place, operation)?;
        self.continue_property_mutation(path, completion).await
    }

    pub(crate) async fn reference_property_path(
        &mut self,
        place: AccessPath,
        completion: ReferenceCompletion,
    ) -> Result<(), NativeError> {
        let path = self.start_magic_reference(place)?;
        self.continue_property_reference(path, completion).await
    }

    async fn continue_property_reference(
        &mut self,
        mut path: ReferencePath,
        completion: ReferenceCompletion,
    ) -> Result<(), NativeError> {
        match self.advance_magic_reference(&mut path).await? {
            ReferenceStep::Complete(value) => completion.finish(self, value),
            ReferenceStep::Hook(function, getter) => {
                self.suspend_for_hook(
                    function,
                    getter.object.clone(),
                    None,
                    HookContinuation::Reference {
                        path,
                        completion,
                        getter,
                    },
                )
                .await
            }
        }
    }

    async fn continue_property_mutation(
        &mut self,
        mut path: MutationPath,
        completion: WriteCompletion,
    ) -> Result<(), NativeError> {
        match self.advance_magic_mutation(&mut path).await? {
            MutationStep::Complete(converted) => {
                completion.finish(self, converted);
                Ok(())
            }
            MutationStep::Hook(MutationHook::Getter {
                function,
                object,
                name,
                last,
                indirect,
            }) => {
                let getter = MutationGetter {
                    object: object.clone(),
                    name,
                    last,
                    indirect,
                };
                self.suspend_for_hook(
                    function,
                    object,
                    None,
                    HookContinuation::Mutation {
                        path,
                        completion,
                        getter,
                    },
                )
                .await
            }
            MutationStep::Hook(MutationHook::Setter {
                function,
                object,
                value,
                local,
            }) => {
                self.suspend_for_hook(
                    function,
                    object,
                    Some(value),
                    HookContinuation::Written { local, completion },
                )
                .await
            }
        }
    }

    pub(crate) async fn suspend_for_hook(
        &mut self,
        function: FunctionIndex,
        receiver: Rc<Object>,
        argument: Option<Value>,
        continuation: HookContinuation,
    ) -> Result<(), NativeError> {
        if !self.enter_exact_method(function, &receiver, argument.as_ref(), None)? {
            self.prepare_declared_hook(function, receiver, argument)?;
            let origin = ArgumentOrigin::Php;
            let mut entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
            if matches!(entry, CallEntry::ConvertStrings) {
                self.coerce_call_strings(origin).await?;
                entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
            }
            if !matches!(entry, CallEntry::Php) {
                return Err(internal_error!("property hook entry"));
            }
        }
        self.call_stack.current_extras_mut().property_hook = Some(Box::new(continuation));
        Ok(())
    }
}
