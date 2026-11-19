//! `openssl_public_encrypt($data, $encrypted, $publicKey, OPENSSL_PKCS1_OAEP_PADDING)`
//! encrypts a short message for the private key holder.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Value, args,
};
use rsa::sha2::digest::{Digest, FixedOutputReset};
use rsa::{Oaep, Pkcs1v15Sign};

use crate::digest::Algorithm;
use crate::errors;
use crate::key::{Key, OsRandom, Private, Public};
use crate::key_input::{self, Role};

const DATA: Param = Param::new("data", ParamType::String);
const ENCRYPTED: Param = Param::new("encrypted_data", ParamType::Mixed).by_ref();
const DECRYPTED: Param = Param::new("decrypted_data", ParamType::Mixed).by_ref();
const PUBLIC_KEY: Param = Param::new("public_key", ParamType::Mixed);
const PRIVATE_KEY: Param = Param::new("private_key", ParamType::Mixed);
const PADDING: Param = Param::new("padding", ParamType::Int).with_default(ConstantValue::Int(1));
const DIGEST: Param = Param::new("digest_algo", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);
const PUBLIC_ENCRYPT: &str = "openssl_public_encrypt";
const PRIVATE_DECRYPT: &str = "openssl_private_decrypt";
const PRIVATE_ENCRYPT: &str = "openssl_private_encrypt";

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous(
        PUBLIC_ENCRYPT,
        &[DATA, ENCRYPTED, PUBLIC_KEY, PADDING, DIGEST],
        public_encrypt,
    ),
    FunctionEntry::asynchronous(
        PRIVATE_DECRYPT,
        &[DATA, DECRYPTED, PRIVATE_KEY, PADDING, DIGEST],
        private_decrypt,
    ),
    FunctionEntry::asynchronous(
        PRIVATE_ENCRYPT,
        &[DATA, ENCRYPTED, PRIVATE_KEY, PADDING],
        private_encrypt,
    ),
];

enum Operation<'a> {
    PublicEncrypt(&'a rsa::PublicKey),
    PrivateDecrypt(&'a rsa::PrivateKey),
    PrivateEncrypt(&'a rsa::PrivateKey),
}

impl Operation<'_> {
    fn pkcs1(&self, data: &[u8]) -> rsa::Result<Vec<u8>> {
        match self {
            Self::PublicEncrypt(key) => key.pkcs1v15_encrypt_with_rng(&mut OsRandom, data),
            Self::PrivateDecrypt(key) => key.pkcs1v15_decrypt_with_rng(&mut OsRandom, data),
            Self::PrivateEncrypt(key) => {
                key.sign_with_rng(&mut OsRandom, Pkcs1v15Sign::new_unprefixed(), data)
            }
        }
    }

    fn oaep(&self, data: &[u8], algorithm: Algorithm) -> rsa::Result<Vec<u8>> {
        use rsa::sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
        match algorithm {
            Algorithm::Md5 => self.apply_oaep::<md5_rsa::Md5>(data),
            Algorithm::Ripemd160 => self.apply_oaep::<ripemd_rsa::Ripemd160>(data),
            Algorithm::Sha1 => self.apply_oaep::<sha1_rsa::Sha1>(data),
            Algorithm::Sha224 => self.apply_oaep::<Sha224>(data),
            Algorithm::Sha256 => self.apply_oaep::<Sha256>(data),
            Algorithm::Sha384 => self.apply_oaep::<Sha384>(data),
            Algorithm::Sha512 => self.apply_oaep::<Sha512>(data),
            Algorithm::Sha512224 => self.apply_oaep::<Sha512_224>(data),
            Algorithm::Sha512256 => self.apply_oaep::<Sha512_256>(data),
            Algorithm::Sha3_224 => self.apply_oaep::<sha3_rsa::Sha3_224>(data),
            Algorithm::Sha3_256 => self.apply_oaep::<sha3_rsa::Sha3_256>(data),
            Algorithm::Sha3_384 => self.apply_oaep::<sha3_rsa::Sha3_384>(data),
            Algorithm::Sha3_512 => self.apply_oaep::<sha3_rsa::Sha3_512>(data),
        }
    }

    fn apply_oaep<D: Digest + FixedOutputReset>(&self, data: &[u8]) -> rsa::Result<Vec<u8>> {
        let padding = Oaep::<D>::new();
        match self {
            Self::PublicEncrypt(key) => key.encrypt(&mut OsRandom, padding, data),
            Self::PrivateDecrypt(key) => key.decrypt_blinded(&mut OsRandom, padding, data),
            Self::PrivateEncrypt(_) => Err(rsa::Error::InvalidPaddingScheme),
        }
    }
}

fn public_encrypt(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { run(context, &arguments, PUBLIC_ENCRYPT, Role::Public).await })
}

fn private_encrypt(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { run(context, &arguments, PRIVATE_ENCRYPT, Role::Private).await })
}

fn private_decrypt(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { run(context, &arguments, PRIVATE_DECRYPT, Role::Private).await })
}

async fn run(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    role: Role,
) -> NativeResult {
    let digest = args::string(arguments, 4).map(|string| string.as_bytes());
    if digest.is_some_and(|name| name.contains(&0)) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #5 ($digest_algo) must not contain any null bytes"
        )));
    }
    let key = key_input::load(context, args::value(arguments, 2), role, b"", function).await?;
    let operation = match key.as_deref() {
        Some(Key::Private(Private::Rsa(key))) if function == PRIVATE_ENCRYPT => {
            Operation::PrivateEncrypt(key)
        }
        Some(Key::Private(Private::Rsa(key))) => Operation::PrivateDecrypt(key),
        Some(Key::Public(Public::Rsa(key))) => Operation::PublicEncrypt(key),
        Some(_) => return Ok(Value::Bool(false)),
        None => {
            let message = match (role, function) {
                (Role::Public, _) => "key parameter is not a valid public key",
                (_, PRIVATE_ENCRYPT) => "key param is not a valid private key",
                _ => "key parameter is not a valid private key",
            };
            context.warning(format!("{function}(): {message}"));
            return Ok(Value::Bool(false));
        }
    };
    let padding = args::int(arguments, 3).unwrap_or(1);
    if !matches!(padding, 1 | 4) || padding == 4 && function == PRIVATE_ENCRYPT {
        errors::push(context, "error:02000076:rsa routines::unknown padding type");
        return Ok(Value::Bool(false));
    }
    let algorithm = if let Some(name) = digest {
        let Some(algorithm) = Algorithm::lookup(name) else {
            let name = String::from_utf8_lossy(name);
            context.warning(format!("{function}(): Unknown digest algorithm: {name}"));
            return Ok(Value::Bool(false));
        };
        algorithm
    } else {
        Algorithm::Sha1
    };
    let data = args::bytes(arguments, 0);
    let output = if padding == 1 {
        operation.pkcs1(data)
    } else {
        operation.oaep(data, algorithm)
    };
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            let message = match error {
                rsa::Error::MessageTooLong => {
                    "error:0200006E:rsa routines::data too large for key size"
                }
                rsa::Error::Decryption if padding == 4 => {
                    "error:02000079:rsa routines::oaep decoding error"
                }
                _ => "error:0200006F:rsa routines::data too small",
            };
            errors::push(context, message);
            return Ok(Value::Bool(false));
        }
    };
    if let Some(reference) = args::reference(arguments, 1) {
        *reference.borrow_mut() = Value::from(output);
    }
    Ok(Value::Bool(true))
}
