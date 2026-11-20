//! `readdir(opendir('virtual://folder'))` calls `dir_readdir()` without buffering entries.

use std::{cell::Cell, rc::Rc};

use phera_extension_api::{
    Context, NativeError, NativeResult, Object, Value,
    value::{Resource, to_bool},
};

use crate::user_file::{invoke, missing};
use crate::user_wrappers::UserWrapper;

pub(crate) struct UserDirectory {
    pub object: Rc<Object>,
    pub remote: bool,
    busy: Cell<bool>,
}

pub(crate) async fn open(
    context: &mut dyn Context,
    wrapper: UserWrapper,
    function: &str,
    uri: &[u8],
    supplied: Value,
) -> Result<Option<Rc<UserDirectory>>, NativeError> {
    if !wrapper.permitted(context, function) {
        return Ok(None);
    }
    let object = wrapper.object(context, supplied).await?;
    let opened = invoke(
        context,
        &object,
        "dir_opendir",
        vec![crate::user_paths::path(uri), Value::Int(0)],
    )
    .await?;
    if opened.is_none() {
        missing(context, &object, function, "dir_opendir", "");
    }
    Ok(opened.filter(to_bool).map(|_| {
        Rc::new(UserDirectory {
            object,
            remote: wrapper.remote,
            busy: Cell::new(false),
        })
    }))
}

pub(crate) fn of(resource: &Resource) -> Option<Rc<UserDirectory>> {
    resource
        .data
        .borrow()
        .as_ref()?
        .downcast_ref::<Rc<UserDirectory>>()
        .cloned()
}

impl UserDirectory {
    pub async fn read(&self, context: &mut dyn Context, function: &str) -> NativeResult {
        let _busy = self.enter()?;
        match invoke(context, &self.object, "dir_readdir", Vec::new()).await? {
            None => {
                missing(context, &self.object, function, "dir_readdir", "");
                Ok(Value::Bool(false))
            }
            Some(Value::Bool(_)) => Ok(Value::Bool(false)),
            Some(value) => {
                let name = context.stringify(&value).await?;
                let bytes = name
                    .as_bytes()
                    .split(|byte| *byte == 0)
                    .next()
                    .unwrap_or_default();
                Ok(Value::from(bytes[..bytes.len().min(255)].to_vec()))
            }
        }
    }

    pub async fn rewind(&self, context: &mut dyn Context) -> NativeResult {
        let _busy = self.enter()?;
        invoke(context, &self.object, "dir_rewinddir", Vec::new()).await?;
        Ok(Value::Null)
    }

    pub async fn close(&self, context: &mut dyn Context) -> NativeResult {
        let _busy = self.enter()?;
        invoke(context, &self.object, "dir_closedir", Vec::new()).await?;
        Ok(Value::Null)
    }

    pub fn is_busy(&self) -> bool {
        self.busy.get()
    }

    fn enter(&self) -> Result<Busy<'_>, NativeError> {
        if self.busy.replace(true) {
            return Err(NativeError::error(
                "Directory is already executing a callback",
            ));
        }
        Ok(Busy(&self.busy))
    }
}

struct Busy<'a>(&'a Cell<bool>);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
