//! Native method harness: `new PDO($dsn)` through the public extension registry.

use phera_ext_pdo::Pdo;
use phera_extension_api::testing::{TestContext, instantiate};
use phera_extension_api::{
    ClassEntry, NativeMethod, NativeResult, Object, Registry, Value, prepare_call,
};
use std::rc::Rc;

pub struct Harness {
    registry: Registry,
    context: TestContext,
}

impl Harness {
    pub fn new() -> Self {
        let mut registry = Registry::new();
        registry.class(ClassEntry::class("RuntimeException"));
        registry.load(&Pdo);
        Self {
            registry,
            context: TestContext::default(),
        }
    }

    pub async fn call(
        &mut self,
        object: &Rc<Object>,
        name: &str,
        mut args: Vec<Value>,
    ) -> NativeResult {
        let class = self.registry.get_class(object.class.name()).unwrap();
        let method = class
            .methods
            .iter()
            .find(|method| method.name == name)
            .unwrap();
        prepare_call(
            &format!("{}::{name}", class.name),
            method.params,
            &mut args,
            &mut self.context,
        )?;
        match method.handler {
            NativeMethod::Sync(handler) => handler(&mut self.context, Some(object), &args),
            NativeMethod::Async(handler) => {
                handler(&mut self.context, Some(object.clone()), args).await
            }
            NativeMethod::Window(method) => {
                (method.run)(&mut self.context, Some(object.clone()), args).await
            }
            NativeMethod::Abstract => panic!("abstract method"),
        }
    }

    pub async fn connect(&mut self, driver: &str) -> Rc<Object> {
        self.connect_options(driver, phera_extension_api::PhpArray::new())
            .await
            .unwrap()
    }

    pub async fn connect_options(
        &mut self,
        driver: &str,
        options: phera_extension_api::PhpArray,
    ) -> Result<Rc<Object>, phera_extension_api::NativeError> {
        let object = instantiate(&self.registry, "PDO").unwrap();
        self.call(
            &object,
            "__construct",
            vec![
                Value::from(super::support::dsn(driver)),
                Value::from("phera"),
                Value::from("phera"),
                Value::from(options),
            ],
        )
        .await?;
        Ok(object)
    }
}

pub fn object(value: Value) -> Rc<Object> {
    let Value::Object(object) = value else {
        panic!("Expected object: {value:?}");
    };
    object
}
