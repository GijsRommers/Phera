//! `openssl_pkey_get_private([$pem, $passphrase])` reads an encrypted private key.

use std::rc::Rc;

use phera_extension_api::{ArrayKey, Context, NativeError, Value};

use crate::errors;
use crate::key::{self, Key};
use crate::key_format;

#[derive(Clone, Copy)]
pub(crate) enum Role {
    Private,
    Public,
}

pub(crate) async fn load(
    context: &mut dyn Context,
    value: &Value,
    role: Role,
    password: &[u8],
    function: &str,
) -> Result<Option<Rc<Key>>, NativeError> {
    let value = value.deref().clone();
    let array_password;
    let (value, password) = if let Value::Array(array) = &value {
        let invalid = || {
            NativeError::value_error("Key array must be of the form array(0 => key, 1 => phrase)")
        };
        let key = array.get(&ArrayKey::Int(0)).ok_or_else(invalid)?;
        let phrase = array.get(&ArrayKey::Int(1)).ok_or_else(invalid)?;
        if matches!(role, Role::Private) {
            array_password = context.stringify(phrase).await?;
            (key, array_password.as_bytes())
        } else {
            (key, password)
        }
    } else {
        (&value, password)
    };
    let value = value.deref().clone();
    if let Value::Object(object) = &value
        && let Some(key) = key::stored(object)
    {
        return match (&*key, role) {
            (Key::Private(_), Role::Private) | (Key::Public(_), Role::Public) => Ok(Some(key)),
            (Key::Private(_), Role::Public) => {
                context.warning(format!(
                    "{function}(): Don't know how to get public key from this private key"
                ));
                Ok(None)
            }
            (Key::Public(_), Role::Private) => Ok(None),
        };
    }
    let string = match value {
        Value::String(string) => string,
        Value::Object(_) => context.stringify(&value).await?,
        _ => return Ok(None),
    };
    let bytes = string.as_bytes();
    let file;
    let bytes = if let Some(path) = bytes.strip_prefix(b"file://") {
        let Some(path) = path_from_bytes(path) else {
            return Ok(None);
        };
        match tokio::fs::read(path).await {
            Ok(contents) => {
                file = contents;
                &file
            }
            Err(_) => {
                errors::push(
                    context,
                    "error:FFFFFFFF80000002:system library::No such file or directory",
                );
                errors::push(context, "error:10000080:BIO routines::no such file");
                return Ok(None);
            }
        }
    } else {
        bytes
    };
    let parsed = match role {
        Role::Private => key_format::private(bytes, password).map(Key::Private),
        Role::Public => key_format::public(bytes).map(Key::Public),
    };
    if parsed.is_none() {
        errors::push(context, "error:1E08010C:DECODER routines::unsupported");
    }
    Ok(parsed.map(Rc::new))
}

pub(crate) fn path_from_bytes(bytes: &[u8]) -> Option<&std::path::Path> {
    if bytes.contains(&0) {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(std::path::Path::new(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        Some(std::path::Path::new(std::str::from_utf8(bytes).ok()?))
    }
}
