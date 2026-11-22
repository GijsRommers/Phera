//! A getter in `$object->{$name}->child` leaves its cursor on a PHP frame.
//! Its return resumes the walk from that cursor, with no nested VM call.

use std::rc::Rc;

use phera_bytecode::{BinaryOp, FetchMode, IncDec, IterId, Reg};
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
use crate::magic_references::{ReferenceGetter, ReferencePath, ReferenceStep, ReferenceUse};
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
    Foreach(IterId, Reg),
}

impl ReferenceCompletion {
    async fn finish(self, vm: &mut Vm, value: Value) -> Result<(), NativeError> {
        match self {
            Self::Fetch(dst) => {
                vm.set(dst, value);
                Ok(())
            }
            Self::Argument(index) => vm.store_sent_value(index, value),
            Self::Foreach(iterator, subject) => {
                vm.finish_reference_foreach(iterator, subject, value).await
            }
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
    Lazy(
        crate::lazy_objects::Initialization,
        crate::lazy_continuation::LazyContinuation,
    ),
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
    ReflectedCall(Reg, crate::error_traces::NativeTrace),
    Clone(Reg, crate::classes::cloning::CloneUpdates),
    ObjectVars(
        crate::hooked_property_views::ObjectVars,
        phera_value::ArrayKey,
    ),
    NativeView(
        Box<crate::native_property_views::NativeView>,
        phera_value::ArrayKey,
    ),
}

impl HookContinuation {
    pub(crate) fn native_trace(&self) -> Option<&crate::error_traces::NativeTrace> {
        match self {
            Self::Lazy(_, continuation) => continuation.native_trace(),
            Self::ObjectVars(view, _) => Some(&view.native),
            Self::NativeView(view, _) => Some(&view.native),
            Self::ReflectedCall(_, native) => Some(native),
            _ => None,
        }
    }
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
            HookContinuation::Lazy(initialization, continuation) => {
                use crate::lazy_continuation::LazyContinuation;
                let value = initialization.finish(returned.value)?;
                match continuation {
                    LazyContinuation::Read(path, completion) => {
                        self.continue_property_path(path, completion).await
                    }
                    LazyContinuation::Mutation(path, completion) => {
                        self.continue_property_mutation(path, completion).await
                    }
                    LazyContinuation::Reference(path, completion) => {
                        self.continue_property_reference(path, completion).await
                    }
                    LazyContinuation::Iterate(iterator, subject, object) => {
                        self.lazy_iterator(iterator, subject, object).await
                    }
                    LazyContinuation::Clone(destination, object, properties) => {
                        self.start_clone(destination, Value::Object(object), properties)
                            .await
                    }
                    LazyContinuation::PropertyExists(destination, object, name, _) => {
                        self.lazy_property_exists(destination, object, name).await
                    }
                    LazyContinuation::Comparison(destination, operator, comparison) => {
                        let work = self.step_comparison(|diagnostics, objects| {
                            comparison.resume(objects, diagnostics)
                        })?;
                        self.continue_comparison(crate::string_comparison::PendingComparison {
                            destination,
                            operator,
                            work,
                        })
                        .await
                    }
                    LazyContinuation::NativeView(destination, request, _) => {
                        if let Some(value) = self.drive_view_request(destination, *request).await? {
                            self.set_call_result(destination, value);
                        }
                        Ok(())
                    }
                    LazyContinuation::ObjectVars(destination, object, _) => {
                        self.lazy_object_vars(destination, object).await
                    }
                    LazyContinuation::Reflection(destination, operation, _) => {
                        let value = self.resume_reflected_lazy(*operation, value)?;
                        self.set_call_result(destination, value);
                        Ok(())
                    }
                }
            }
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
                    completion.finish(self, value).await
                } else {
                    self.continue_property_reference(path, completion).await
                }
            }
            HookContinuation::Iteration(getter) => {
                self.resume_hooked_iteration(getter, returned.value)
            }
            HookContinuation::Clone(destination, updates) => {
                self.continue_clone_updates(destination, updates).await
            }
            HookContinuation::ObjectVars(view, key) => {
                self.resume_object_vars(view, key, returned.value).await
            }
            HookContinuation::ReflectedCall(destination, _) => {
                self.set(destination, returned.value.deref().clone());
                Ok(())
            }
            HookContinuation::NativeView(view, key) => {
                self.resume_native_view(view, key, returned.value).await
            }
        }
    }

    async fn continue_property_path(
        &mut self,
        mut path: PathRead,
        completion: ReadCompletion,
    ) -> Result<(), NativeError> {
        match self.advance_path_read(&mut path).await? {
            PathReadStep::Lazy(object) => {
                let continuation =
                    crate::lazy_continuation::LazyContinuation::Read(path, completion);
                self.suspend_lazy_initialization(object, continuation)
                    .await?;
                Ok(())
            }
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
        let use_ = match completion {
            ReferenceCompletion::Foreach(..) => ReferenceUse::Foreach,
            _ => ReferenceUse::Alias,
        };
        let path = self.start_magic_reference(place, use_)?;
        self.continue_property_reference(path, completion).await
    }

    async fn continue_property_reference(
        &mut self,
        mut path: ReferencePath,
        completion: ReferenceCompletion,
    ) -> Result<(), NativeError> {
        match self.advance_magic_reference(&mut path).await? {
            ReferenceStep::Lazy(object) => {
                let continuation =
                    crate::lazy_continuation::LazyContinuation::Reference(path, completion);
                self.suspend_lazy_initialization(object, continuation)
                    .await?;
                Ok(())
            }
            ReferenceStep::Complete(value) => completion.finish(self, value).await,
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
            MutationStep::Lazy(object) => {
                let continuation =
                    crate::lazy_continuation::LazyContinuation::Mutation(path, completion);
                self.suspend_lazy_initialization(object, continuation)
                    .await?;
                Ok(())
            }
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
