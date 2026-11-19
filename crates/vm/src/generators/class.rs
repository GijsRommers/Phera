//! Generator's public methods and the iterator cursor that shares their driver.
//!
//! ```php
//! $generator = (function () { yield 1; })();
//! var_dump($generator instanceof Iterator);
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, GeneratorOperation, LocalFuture, MethodEntry,
    NativeClone, NativeError, NativeResult, Param, ParamType, PropertyPurpose, WindowMethod,
    traversal::{IteratorStep, StepFuture, Traversal},
};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use super::{State, state};

pub(crate) const INSTANTIATION_ERROR: &str =
    "The \"Generator\" class is reserved for internal use and cannot be manually instantiated";

const SEND: &[Param] = &[Param::new("value", ParamType::Mixed)];
const THROW: &[Param] = &[Param::new("exception", ParamType::Class("Throwable"))];

const SEND_METHOD: WindowMethod = WindowMethod {
    try_ready: |cx, object, value| {
        cx.generator_ready(object, &GeneratorOperation::Send(value?.clone()))
    },
    run: |cx, object, mut arguments| {
        run(
            cx,
            object,
            GeneratorOperation::Send(arguments.pop().unwrap_or_default()),
        )
    },
};

pub(crate) const ENTRY: ClassEntry = ClassEntry {
    kind: ClassKind::FinalClass,
    interfaces: &["Iterator"],
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Forbidden),
        properties: Some(properties),
        ..ClassHooks::EMPTY
    }),
    methods: &[
        MethodEntry::public_async("rewind", &[], |cx, object, _| {
            run(cx, object, GeneratorOperation::Method(IteratorStep::Rewind))
        })
        .returning("void"),
        MethodEntry::public_async("valid", &[], |cx, object, _| {
            run(cx, object, GeneratorOperation::Method(IteratorStep::Valid))
        })
        .returning("bool"),
        MethodEntry::public_async("current", &[], |cx, object, _| {
            run(
                cx,
                object,
                GeneratorOperation::Method(IteratorStep::Current),
            )
        })
        .returning("mixed"),
        MethodEntry::public_async("key", &[], |cx, object, _| {
            run(cx, object, GeneratorOperation::Method(IteratorStep::Key))
        })
        .returning("mixed"),
        MethodEntry::public_async("next", &[], |cx, object, _| {
            run(
                cx,
                object,
                GeneratorOperation::Method(IteratorStep::Advance),
            )
        })
        .returning("void"),
        MethodEntry::public_window("send", SEND, &SEND_METHOD).returning("mixed"),
        MethodEntry::public_async("throw", THROW, |cx, object, mut arguments| {
            run(
                cx,
                object,
                GeneratorOperation::Throw(arguments.pop().unwrap_or_default()),
            )
        })
        .returning("mixed"),
        MethodEntry::public_async("getReturn", &[], |cx, object, _| {
            run(cx, object, GeneratorOperation::GetReturn)
        })
        .returning("mixed"),
        MethodEntry::public("__debugInfo", &[], debug_info).returning("array"),
    ],
    ..ClassEntry::class("Generator")
};

pub(crate) const CLOSED_EXCEPTION: ClassEntry = ClassEntry {
    parent: Some("Exception"),
    ..ClassEntry::class("ClosedGeneratorException")
};

fn run(
    cx: &mut dyn Context,
    object: Option<Rc<Object>>,
    operation: GeneratorOperation,
) -> LocalFuture<'_, NativeResult> {
    match object {
        Some(object) => cx
            .generator_operation(object, operation)
            .into_local_future(),
        None => Box::pin(std::future::ready(Err(internal_error!(
            "Generator method receiver"
        )))),
    }
}

fn debug_info(cx: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let object = object.ok_or_else(|| internal_error!("Generator debug receiver"))?;
    Ok(Value::Array(Rc::new(properties(
        cx,
        object,
        PropertyPurpose::Debug,
    )?)))
}

fn properties(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut properties = PhpArray::new();
    if matches!(purpose, PropertyPurpose::Debug) {
        let name = state(object, |generator| generator.name.clone())?;
        properties.insert(ArrayKey::str("function"), name);
    }
    if matches!(purpose, PropertyPurpose::Serialize) {
        return Err(NativeError::throw(
            "Exception",
            "Serialization of 'Generator' is not allowed",
        ));
    }
    Ok(properties)
}

pub(crate) struct Cursor(Rc<Object>, bool);

impl Cursor {
    pub(crate) fn open(object: Rc<Object>, by_reference: bool) -> Result<Self, NativeError> {
        state(&object, |generator| {
            if matches!(generator.state, State::Returned(_) | State::Failed) {
                Err(NativeError::throw(
                    "Exception",
                    "Cannot traverse an already closed generator",
                ))
            } else if by_reference && !generator.reference {
                Err(NativeError::throw(
                    "Exception",
                    "You can only iterate a generator by-reference if it declared that it yields by-reference",
                ))
            } else {
                Ok(())
            }
        })??;
        Ok(Self(object, by_reference))
    }
}

impl Traversal for Cursor {
    fn generator(&self) -> Option<&Rc<Object>> {
        Some(&self.0)
    }

    fn step<'a>(&'a mut self, context: &'a mut dyn Context, step: IteratorStep) -> StepFuture<'a> {
        context.generator_operation(
            self.0.clone(),
            GeneratorOperation::Cursor {
                step,
                by_reference: self.1,
            },
        )
    }
}
