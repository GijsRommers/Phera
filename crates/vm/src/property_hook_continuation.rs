//! A getter in `$object->{$name}->child` leaves its cursor on a PHP frame.
//! Its return resumes the walk from that cursor, with no nested VM call.

use std::rc::Rc;

use phera_bytecode::{BinaryOp, FetchMode, IncDec, Reg};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::calls::call_execution::CallEntry;
use crate::code_table::FunctionIndex;
use crate::magic_places::{PathRead, PathReadStep};
use crate::{AccessPath, Vm};

pub(crate) enum ReadCompletion {
    Fetch(Reg),
    Argument(usize),
    AssignOp(BinaryOp, Reg, Reg),
    IncDec(IncDec, Reg),
}

pub(crate) struct HookContinuation {
    path: PathRead,
    completion: ReadCompletion,
    indirect: Option<(Rc<Object>, String)>,
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
        let HookContinuation {
            mut path,
            completion,
            indirect,
        } = *returned.continuation;
        self.resume_path_read(&mut path, returned.value, indirect)?;
        self.continue_property_path(path, completion).await
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
                    HookContinuation {
                        path,
                        completion,
                        indirect,
                    },
                )
            }
        }
    }

    fn suspend_for_hook(
        &mut self,
        function: FunctionIndex,
        receiver: Rc<Object>,
        argument: Option<Value>,
        continuation: HookContinuation,
    ) -> Result<(), NativeError> {
        if !self.enter_exact_method(function, &receiver, argument.as_ref(), None)? {
            self.prepare_declared_hook(function, receiver, argument)?;
            if !matches!(self.enter_prepared_call(None)?, CallEntry::Php) {
                return Err(internal_error!("property hook entry"));
            }
        }
        self.call_stack.current_extras_mut().property_hook = Some(Box::new(continuation));
        Ok(())
    }
}
