//! A modern O: object embedded in a legacy SPL payload receives the decoded
//! member array after the container has read all of its data.
//!
//! ```php
//! $object = new ArrayObject([new SplFixedArray(1)]);
//! $object->unserialize($legacyPayload);
//! ```

use std::rc::Rc;

use phera_extension_api::serialization::ActiveUnserializePolicy;
use phera_extension_api::{Context, NativeError, Object, PhpArray, Value};

use super::Reader;

enum Hook {
    Wakeup(Rc<Object>),
    Unserialize(Rc<Object>, PhpArray),
}

#[derive(Default)]
pub(super) struct Hooks(Vec<Hook>);

impl Hooks {
    pub(super) fn wakeup(&mut self, object: Rc<Object>) {
        self.0.push(Hook::Wakeup(object));
    }

    pub(super) fn unserialize(&mut self, object: Rc<Object>, arguments: PhpArray) {
        self.0.push(Hook::Unserialize(object, arguments));
    }

    async fn invoke(self, context: &mut dyn Context) -> Result<(), NativeError> {
        for hook in self.0 {
            let (object, method, arguments) = match hook {
                Hook::Wakeup(object) => (object, "__wakeup", Vec::new()),
                Hook::Unserialize(object, argument) => {
                    (object, "__unserialize", vec![Value::from(argument)])
                }
            };
            context
                .call_serialization_method(object, method, arguments)
                .await?;
        }
        Ok(())
    }
}

impl Reader<'_> {
    pub(crate) async fn finish(self, context: &mut dyn Context) -> Result<(), NativeError> {
        if self.hooks.0.is_empty() {
            return Ok(());
        }
        let previous = ActiveUnserializePolicy::replace(context, None);
        let result = self.hooks.invoke(context).await;
        ActiveUnserializePolicy::replace(context, previous);
        result
    }
}
