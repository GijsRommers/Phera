//! `iterator_count($range)` moves a native cursor without reading its values.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, Context, FunctionEntry, MethodEntry, NativeClone, NativeError,
    NativeIterator, NativeResult, Object, Param, ParamType, Registry, Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate"],
        methods: const { &[MethodEntry::public("getIterator", &[], get_iterator)] },
        hooks: const {
            Some(&ClassHooks {
                clone: Some(NativeClone::Forbidden),
                iterator: Some(cursor),
                ..ClassHooks::EMPTY
            })
        },
        ..ClassEntry::class("EngineRange")
    });
    registry.function(FunctionEntry::sync(
        "uncloneable",
        const { &[Param::new("range", ParamType::Mixed)] },
        identity,
    ));
}

fn identity(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::value(arguments, 0).clone())
}

fn cursor(_: &mut dyn Context, _: Rc<Object>) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Range { position: 0 }))
}

fn get_iterator(context: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    context.create_iterator(Box::new(Range { position: 0 }))
}

struct Range {
    position: i64,
}

impl NativeIterator for Range {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.position = 0;
        Ok(())
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.position = self.position.saturating_add(1);
        Ok(())
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        Ok(self.position < 3)
    }

    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        context.write(format!("value {}\n", self.position).as_bytes());
        Ok(Value::Int(self.position.saturating_add(3)))
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(Value::Int(self.position.saturating_add(10)))
    }
}
