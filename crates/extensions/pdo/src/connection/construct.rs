//! `new PDO($dsn, $user, $password, $options)` borrows a configured connection.

use crate::connect_options::FromPhp;
use crate::dsn::ParseDsn;
use crate::pool::FromOptions;
use phera_db_drivers::connect_options::ConnectOptions;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, Value, args,
};

use super::Session;
use crate::attributes::{self, AttributeCall, Attributes};
use crate::constants::{ATTR_ERRMODE, ATTR_PERSISTENT};
use crate::dsn::{DriverName, Dsn};
use crate::errors::{ErrorMode, ErrorState};
use crate::pool::{self, Policy};

pub fn construct<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    construct_with_call(context, this, arguments, AttributeCall::Construct)
}

pub(super) fn construct_with_call<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    call: AttributeCall,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = this.ok_or_else(|| NativeError::error("PDO object is missing"))?;
        if object.native.borrow().is_some() {
            return Err(NativeError::error("PDO object is already initialized"));
        }
        let dsn_text = args::string(&arguments, 0)
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let mut dsn = Dsn::parse_named(&dsn_text, call.method())?;
        let username = args::string(&arguments, 1)
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| dsn.get("user", "").to_owned());
        let password = args::string(&arguments, 2)
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| dsn.get("password", "").to_owned());
        let policy = Policy::from_options(args::array(&arguments, 3).map(Rc::as_ref));
        let mut attributes = Attributes::new(dsn.driver);
        if let Some(options) = args::array(&arguments, 3) {
            if let Some(value) = options.get(&ArrayKey::Int(ATTR_PERSISTENT)) {
                use phera_extension_api::value::{is_numeric, to_int};
                attributes.persistent = match &*value.deref() {
                    Value::InternedStr(text)
                        if !text.as_bytes().is_empty() && !is_numeric(text.as_bytes()) =>
                    {
                        true
                    }
                    Value::String(text)
                        if !text.as_bytes().is_empty() && !is_numeric(text.as_bytes()) =>
                    {
                        true
                    }
                    value => to_int(value, context.diagnostics())? != 0,
                };
            }
            if let Some(value) = options.get(&ArrayKey::Int(ATTR_ERRMODE)) {
                attributes.errors =
                    match phera_extension_api::value::to_int(value, context.diagnostics())? {
                        0 => ErrorMode::Silent,
                        1 => ErrorMode::Warning,
                        _ => ErrorMode::Exception,
                    };
            }
            if dsn.driver == DriverName::Sqlite {
                for (key, value) in options.iter() {
                    if let ArrayKey::Int(attribute) = key {
                        attributes::sqlite_pre_open(context, &mut attributes, attribute, value)?;
                    }
                }
            } else {
                dsn.options = ConnectOptions::from_php(context, dsn.driver, options)?;
                attributes.autocommit = dsn.options.mysql.autocommit;
            }
        }
        if dsn.driver == DriverName::Sqlite {
            // The driver opens the file with the options' flags and timeout.
            dsn.fields
                .insert("flags".into(), attributes.sqlite_flags.to_string());
            dsn.fields
                .insert("timeout".into(), attributes.timeout.to_string());
        }
        let boot = context.is_snapshot_boot();
        let driver = match pool::acquire(&dsn, &username, &password, policy, boot).await {
            Ok(driver) => driver,
            Err(error) => {
                let code = error.code.unwrap_or(0);
                return Err(crate::errors::exception(
                    format!("SQLSTATE[{}] [{code}] {}", error.state, error.detail),
                    Some(&error),
                    code,
                ));
            }
        };
        let version = driver.version().to_owned();
        let connection = Rc::new(Session {
            driver,
            name: dsn.driver,
            version,
            attributes: RefCell::new(attributes),
            error: RefCell::new(ErrorState::Unused),
            last_error: RefCell::new(None),
            transaction: Cell::new(false),
            statement_sequence: Cell::new(0),
        });
        if let Some(options) = args::array(&arguments, 3) {
            for (key, value) in options.iter() {
                if let ArrayKey::Int(attribute) = key {
                    attributes::apply(context, &connection, attribute, value, call).await?;
                }
            }
        }
        *object.native.borrow_mut() = Some(Box::new(connection));
        Ok(Value::Null)
    })
}
