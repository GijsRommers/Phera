//! `openssl_pkey_get_private([$pem, $passphrase])` reads an encrypted private key.

use std::borrow::Cow;
use std::rc::Rc;

use phera_extension_api::{ArrayKey, Context, NativeError, Value, WorkingDirectory};

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
    if let Value::Object(object) = &value
        && let Some(certificate) = crate::certificate::stored(object)
    {
        return Ok(match role {
            Role::Public => crate::certificate::public_key(&certificate)
                .map(Key::Public)
                .map(Rc::new),
            Role::Private => None,
        });
    }
    let string = match value {
        Value::InternedStr(string) => string.to_php_string(),
        Value::String(string) => string,
        Value::Object(_) => context.stringify(&value).await?,
        _ => return Ok(None),
    };
    let Some(bytes) = read_key_file(context, string.as_bytes()).await else {
        return Ok(None);
    };
    let parsed = match role {
        Role::Private => key_format::private(&bytes, password).map(Key::Private),
        Role::Public => key_format::public(&bytes)
            .or_else(|| {
                crate::certificate::parse(&bytes)
                    .and_then(|certificate| crate::certificate::public_key(&certificate))
            })
            .map(Key::Public),
    };
    if parsed.is_none() {
        errors::push(context, "error:1E08010C:DECODER routines::unsupported");
    }
    Ok(parsed.map(Rc::new))
}

pub(crate) async fn read_key_file<'a>(
    context: &mut dyn Context,
    bytes: &'a [u8],
) -> Option<Cow<'a, [u8]>> {
    let Some(path) = bytes.strip_prefix(b"file://") else {
        return Some(Cow::Borrowed(bytes));
    };
    let path = working_path(context, path_from_bytes(path)?).ok()?;
    match tokio::fs::read(path).await {
        Ok(contents) => Some(Cow::Owned(contents)),
        Err(_) => {
            errors::push(
                context,
                "error:FFFFFFFF80000002:system library::No such file or directory",
            );
            errors::push(context, "error:10000080:BIO routines::no such file");
            None
        }
    }
}

pub(crate) fn working_path(
    context: &mut dyn Context,
    path: &std::path::Path,
) -> std::io::Result<std::path::PathBuf> {
    context
        .state()
        .get_or_default::<WorkingDirectory>()
        .resolve(path)
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
