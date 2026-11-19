//! A suspended `$record->name` resumes with its already evaluated property name.

use std::rc::Rc;

use phera_bytecode::{BinaryOp, IterId, Reg};
use phera_extension_api::reflection::{MemberInfo, PropertyRead, PropertyWrite};
use phera_extension_api::{Context, NativeError, PropertyPurpose, Unwind};
use phera_value::{Object, Value};

use crate::Vm;
use crate::error_traces::NativeTrace;
use crate::magic_mutations::MutationPath;
use crate::magic_places::PathRead;
use crate::magic_references::ReferencePath;
use crate::property_hook_continuation::{ReadCompletion, ReferenceCompletion, WriteCompletion};

pub(crate) enum LazyContinuation {
    Read(PathRead, ReadCompletion),
    Mutation(MutationPath, WriteCompletion),
    Reference(ReferencePath, ReferenceCompletion),
    Iterate(IterId, Option<Reg>, Rc<Object>),
    PropertyExists(Reg, Rc<Object>, phera_value::PhpString, NativeTrace),
    Clone(Reg, Rc<Object>, Option<Value>),
    Comparison(Reg, BinaryOp, Box<phera_value::LazyComparison>),
    NativeView(
        Reg,
        Box<phera_extension_api::PropertyViewRequest>,
        NativeTrace,
    ),
    ObjectVars(
        Reg,
        crate::class_function::ClassFunction,
        Rc<Object>,
        NativeTrace,
    ),
    Reflection(Reg, Box<ReflectedOperation>, NativeTrace),
}

impl LazyContinuation {
    pub(crate) fn native_trace(&self) -> Option<&NativeTrace> {
        match self {
            Self::Reflection(_, _, trace)
            | Self::PropertyExists(_, _, _, trace)
            | Self::NativeView(_, _, trace)
            | Self::ObjectVars(_, _, _, trace) => Some(trace),
            _ => None,
        }
    }
}

pub(crate) enum ReflectedOperation {
    Initialize,
    Read(MemberInfo, PropertyRead),
    Write(MemberInfo, Value, PropertyWrite),
}

pub(crate) struct ReflectInitialize {
    pub(crate) object: Rc<Object>,
    pub(crate) operation: ReflectedOperation,
}

impl ReflectInitialize {
    pub(crate) fn suspend(self) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(self)))
    }

    pub(crate) fn take(error: NativeError) -> Result<Self, NativeError> {
        let NativeError::Unwind(payload) = error else {
            return Err(error);
        };
        payload
            .0
            .downcast::<Self>()
            .map(|call| *call)
            .map_err(|payload| NativeError::Unwind(Unwind(payload)))
    }
}

impl Vm {
    pub(crate) async fn lazy_property_exists(
        &mut self,
        destination: Reg,
        original: Rc<Object>,
        name: phera_value::PhpString,
    ) -> Result<(), NativeError> {
        let object = crate::lazy_objects::resolved_lazy_instance(original.clone());
        let display = name.to_string_lossy();
        let (exists, requires_init) = self.lazy_property_exists_status(&object, &display)?;
        if requires_init {
            let trace = NativeTrace::new(
                "property_exists",
                &[Value::Object(original.clone()), Value::String(name.clone())],
            );
            let continuation = LazyContinuation::PropertyExists(destination, original, name, trace);
            if let Some(returned) = self
                .suspend_lazy_initialization(object, continuation)
                .await?
            {
                Box::pin(self.resume_property_hook(returned)).await?;
            }
        } else {
            self.set(destination, Value::Bool(exists));
        }
        Ok(())
    }

    pub(crate) fn lazy_property_exists_status(
        &self,
        object: &Rc<Object>,
        name: &str,
    ) -> Result<(bool, bool), NativeError> {
        let class = self
            .classes
            .for_object(object)
            .ok_or_else(|| internal_error!("lazy property_exists class"))?;
        let exists = self.class_property_exists(&Value::Object(object.clone()), &class, name);
        let requires_init =
            !exists && object.is_uninitialized_lazy() && !class.methods.contains_key("__isset");
        Ok((exists, requires_init))
    }

    pub(crate) async fn lazy_iterator(
        &mut self,
        iterator: IterId,
        subject: Option<Reg>,
        original: Rc<Object>,
    ) -> Result<(), NativeError> {
        let object = crate::lazy_objects::resolved_lazy_instance(original.clone());
        if object.is_uninitialized_lazy() && !object.class.is_traversable() {
            let continuation = LazyContinuation::Iterate(iterator, subject, original);
            if let Some(returned) = self
                .suspend_lazy_initialization(object, continuation)
                .await?
            {
                Box::pin(self.resume_property_hook(returned)).await?;
            }
            return Ok(());
        }
        if object.class.is_traversable() {
            if let Some(subject) = subject {
                self.set(subject, Value::Null);
            }
            self.start_traversable(iterator, Value::Object(object), subject.is_some())
                .await
        } else if let Some(subject) = subject {
            Box::pin(self.finish_reference_foreach(iterator, subject, Value::Object(object))).await
        } else {
            self.iter_start_value(iterator, Value::Object(object), None)
                .map(|_| ())
        }
    }

    pub(crate) async fn lazy_object_vars(
        &mut self,
        destination: Reg,
        function: crate::class_function::ClassFunction,
        original: Rc<Object>,
    ) -> Result<(), NativeError> {
        use crate::class_function::ClassFunction;
        let object = crate::lazy_objects::resolved_lazy_instance(original.clone());
        if object.is_uninitialized_lazy() {
            let trace = NativeTrace::new(function.name(), &[Value::Object(original.clone())]);
            let continuation = LazyContinuation::ObjectVars(destination, function, original, trace);
            if let Some(returned) = self
                .suspend_lazy_initialization(object, continuation)
                .await?
            {
                Box::pin(self.resume_property_hook(returned)).await?;
            }
            return Ok(());
        }
        let properties = if matches!(function, ClassFunction::GetObjectVars) {
            if let Some(class) = self.hooked_property_class(&object) {
                return self.suspend_object_vars(object, class, destination).await;
            }
            self.visible_properties(&object, PropertyPurpose::ObjectVars)?
        } else {
            self.object_properties(&object, PropertyPurpose::ArrayCast)?
        };
        self.set(destination, Value::from(properties));
        Ok(())
    }

    pub(crate) async fn finish_reflected_lazy(
        &mut self,
        request: ReflectInitialize,
        method: &crate::classes::Method,
        destination: Option<Reg>,
    ) -> Result<Option<Value>, NativeError> {
        let ReflectInitialize { object, operation } = request;
        let Some(destination) = destination else {
            let object = self.initialize_lazy_via_callback(object).await?;
            return self
                .resume_reflected_lazy(operation, Value::Object(object))
                .map(Some);
        };
        let trace = NativeTrace::new(
            &format!("{}->{}", method.declaring, method.name),
            &[Value::Object(object.clone())],
        );
        let continuation = LazyContinuation::Reflection(destination, Box::new(operation), trace);
        if let Some(returned) = self
            .suspend_lazy_initialization(object, continuation)
            .await?
        {
            self.resume_property_hook(returned).await?;
        }
        Ok(None)
    }

    pub(crate) fn resume_reflected_lazy(
        &mut self,
        operation: ReflectedOperation,
        value: Value,
    ) -> phera_extension_api::NativeResult {
        use phera_extension_api::reflection::ReflectionContext;
        let Value::Object(object) = value else {
            return Err(internal_error!("reflected lazy result"));
        };
        match operation {
            ReflectedOperation::Initialize => Ok(Value::Object(object)),
            ReflectedOperation::Read(member, mode) => {
                self.reflect_property_read(&member, Some(object), mode)
            }
            ReflectedOperation::Write(member, value, mode) => {
                self.reflect_property_write(&member, Some(object), value, mode)
            }
        }
    }
}
