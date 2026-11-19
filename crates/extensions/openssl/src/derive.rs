//! `openssl_pkey_derive($peerPublicKey, $privateKey)` returns an ECDH secret.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Value, args,
};

use crate::key::{Key, Private, Public};
use crate::key_input::{self, Role};

const NAME: &str = "openssl_pkey_derive";
pub(crate) const ENTRY: FunctionEntry = FunctionEntry::asynchronous(
    NAME,
    &[
        Param::new("public_key", ParamType::Mixed),
        Param::new("private_key", ParamType::Mixed).sensitive(),
        Param::new("key_length", ParamType::Int).with_default(ConstantValue::Int(0)),
    ],
    derive,
);

fn derive(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let length = args::int(&args, 2).unwrap_or_default();
        if args::given(&args, 2) {
            context.deprecated(
                concat!(
                    "openssl_pkey_derive(): the $key_length parameter is deprecated ",
                    "as it is either ignored or truncates the key",
                )
                .into(),
            );
        }
        if length < 0 {
            return Err(NativeError::value_error(
                "openssl_pkey_derive(): Argument #3 ($key_length) must be greater \
                 than or equal to 0",
            ));
        }
        let private =
            key_input::load(context, args::value(&args, 1), Role::Private, b"", NAME).await?;
        let Some(Key::Private(private)) = private.as_deref() else {
            return Ok(Value::Bool(false));
        };
        let public =
            key_input::load(context, args::value(&args, 0), Role::Public, b"", NAME).await?;
        let Some(Key::Public(public)) = public.as_deref() else {
            return Ok(Value::Bool(false));
        };
        let Some(mut shared) = agreement(private, public) else {
            return Ok(Value::Bool(false));
        };
        if length > 0 && length < shared.len() as i64 {
            if matches!(private, Private::X25519(_)) {
                return Ok(Value::Bool(false));
            }
            shared.truncate(length as usize);
        }
        Ok(Value::from(shared))
    })
}

fn agreement(private: &Private, public: &Public) -> Option<Vec<u8>> {
    match (private, public) {
        (Private::P256(private), Public::P256(public)) => {
            Some(private.diffie_hellman(public).raw_secret_bytes().to_vec())
        }
        (Private::P384(private), Public::P384(public)) => {
            Some(private.diffie_hellman(public).raw_secret_bytes().to_vec())
        }
        (Private::P521(private), Public::P521(public)) => {
            Some(private.diffie_hellman(public).raw_secret_bytes().to_vec())
        }
        (Private::X25519(private), Public::X25519(public)) => {
            let secret = private.diffie_hellman(public);
            secret
                .was_contributory()
                .then(|| secret.as_bytes().to_vec())
        }
        _ => None,
    }
}
