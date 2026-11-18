//! `C::A` can depend on `C::B` or on a backed enum case. Pending expressions
//! keep their register windows on the heap while a dependency is computed.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

use crate::{
    Vm,
    classes::{Class, LinkedConstant, enum_cases::BackingBuilder},
    code_table::FunctionIndex,
    initializers::{InitializerFrame, InitializerStep},
};

pub(crate) enum InitializerTask {
    Expression(FunctionIndex, String),
    Constant(ConstantLookup),
    Case(CaseLookup),
    ValidateEnum(Rc<Class>),
}

pub(crate) struct ConstantLookup {
    pub(crate) class: String,
    pub(crate) name: String,
    pub(crate) spelling: Option<String>,
}

pub(crate) struct CaseLookup {
    pub(crate) class: Rc<Class>,
    pub(crate) name: String,
    pub(crate) initializer: Option<FunctionIndex>,
    pub(crate) spelling: Option<String>,
}

pub(crate) enum Continuation {
    Expression(Box<InitializerFrame>, Reg),
    Constant(Rc<LinkedConstant>, String),
    Case(Rc<Class>, String),
    Backing(BackingBuilder),
    AfterValidation(ConstantLookup, Option<String>),
}

pub(crate) enum Progress {
    Done(Value),
    Next(InitializerTask),
    Await(InitializerTask, Continuation),
}

impl Vm {
    pub(crate) fn run_initializer(&mut self, task: InitializerTask) -> NativeResult {
        use Progress::{Await, Done, Next};

        let previous = self.initializer_scope.clone();
        let mut pending = Vec::new();
        let result = (|| {
            let mut progress = Next(task);
            loop {
                progress = match progress {
                    Next(task) => self.start_initializer(task)?,
                    Await(task, continuation) => {
                        pending.push(continuation);
                        Next(task)
                    }
                    Done(value) => match pending.pop() {
                        Some(continuation) => self.resume_initializer(continuation, value)?,
                        None => return Ok(value),
                    },
                };
            }
        })();
        if result.is_err() {
            for continuation in pending.into_iter().rev() {
                match continuation {
                    Continuation::Constant(constant, _) => {
                        Self::cancel_class_constant(&constant);
                    }
                    Continuation::Case(class, name) => {
                        self.cancel_enum_case(&class, name);
                    }
                    Continuation::Backing(builder) => {
                        self.cancel_enum_backing(&builder.class);
                    }
                    Continuation::Expression(..) | Continuation::AfterValidation(..) => {}
                }
            }
        }
        self.initializer_scope = previous;
        result
    }

    fn start_initializer(&mut self, task: InitializerTask) -> Result<Progress, NativeError> {
        match task {
            InitializerTask::Expression(function, scope) => {
                let frame = InitializerFrame::new(&self.code, function, scope);
                self.resume_expression(frame)
            }
            InitializerTask::Constant(lookup) => self.start_class_constant(lookup),
            InitializerTask::Case(lookup) => self.start_enum_case(lookup),
            InitializerTask::ValidateEnum(class) => self.start_enum_backing(class),
        }
    }

    fn resume_initializer(
        &mut self,
        continuation: Continuation,
        value: Value,
    ) -> Result<Progress, NativeError> {
        match continuation {
            Continuation::Expression(mut frame, destination) => {
                frame.store(destination, value);
                self.resume_expression(*frame)
            }
            Continuation::Constant(constant, name) => self
                .finish_class_constant(&constant, &name, value)
                .map(Progress::Done),
            Continuation::Case(class, name) => Ok(Progress::Done(self.finish_enum_case(
                class,
                name,
                Some(value),
            ))),
            Continuation::Backing(builder) => self.resume_enum_backing(builder, value),
            Continuation::AfterValidation(lookup, scope) => {
                self.initializer_scope = scope;
                self.start_class_constant(lookup)
            }
        }
    }

    fn resume_expression(&mut self, mut frame: InitializerFrame) -> Result<Progress, NativeError> {
        self.initializer_scope = Some(frame.scope.clone());
        match self.advance_initializer(&mut frame)? {
            InitializerStep::Done(value) => Ok(Progress::Done(value)),
            InitializerStep::Read(lookup, destination) => Ok(Progress::Await(
                InitializerTask::Constant(lookup),
                Continuation::Expression(Box::new(frame), destination),
            )),
        }
    }
}
