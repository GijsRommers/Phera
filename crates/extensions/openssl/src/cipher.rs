//! `openssl_encrypt('data', 'aes-256-cbc', $key, 0, $iv)` returns Base64.

use phera_ext_standard::{decode_base64, encode_base64};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::aead::{self, Input, Operation};
use crate::block::{self, Direction, Padding};
use crate::constants::{DONT_ZERO_PAD_KEY, RAW_DATA, ZERO_PADDING};
use crate::errors;
use crate::methods::{self, Method, Mode};

const DATA: Param = Param::new("data", ParamType::String);
const CIPHER: Param = Param::new("cipher_algo", ParamType::String);
const KEY: Param = Param::new("passphrase", ParamType::String);
const OPTIONS: Param = Param::new("options", ParamType::Int).with_default(ConstantValue::Int(0));
const IV: Param = Param::new("iv", ParamType::String).with_default(ConstantValue::String(""));
const AAD: Param = Param::new("aad", ParamType::String).with_default(ConstantValue::String(""));
const ENCRYPT: &str = "openssl_encrypt";
const DECRYPT: &str = "openssl_decrypt";

pub(crate) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync(
        ENCRYPT,
        &[
            DATA,
            CIPHER,
            KEY,
            OPTIONS,
            IV,
            Param::new("tag", ParamType::Mixed)
                .by_ref()
                .with_default(ConstantValue::Null),
            AAD,
            Param::new("tag_length", ParamType::Int).with_default(ConstantValue::Int(16)),
        ],
        encrypt,
    ),
    FunctionEntry::sync(
        DECRYPT,
        &[
            DATA,
            CIPHER,
            KEY,
            OPTIONS,
            IV,
            Param::new("tag", ParamType::String)
                .nullable()
                .with_default(ConstantValue::Null),
            AAD,
        ],
        decrypt,
    ),
];

struct Call<'a> {
    function: &'static str,
    direction: Direction,
    method: Method,
    arguments: &'a [Value],
    flags: i64,
}

fn encrypt(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let tag_length = args::int(arguments, 7).unwrap_or(16);
    if i32::try_from(tag_length).is_err() {
        return Err(NativeError::value_error("tag_len is too long"));
    }
    let Some(call) = resolve(context, arguments, Direction::Encrypt) else {
        return Ok(Value::Bool(false));
    };
    let Some(output) = call.run(context, tag_length)? else {
        return Ok(Value::Bool(false));
    };
    if let Some(tag) = args::reference(arguments, 5) {
        *tag.borrow_mut() = output.tag.map_or(Value::Null, Value::from);
    }
    let output = if call.flags & RAW_DATA == 0 {
        encode_base64(&output.data)?
    } else {
        output.data
    };
    Ok(Value::from(output))
}

fn decrypt(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if args::bytes(arguments, 1).is_empty() {
        return Err(methods::empty_cipher(DECRYPT, 2));
    }
    let Some(call) = resolve(context, arguments, Direction::Decrypt) else {
        return Ok(Value::Bool(false));
    };
    Ok(call
        .run(context, 0)?
        .map_or(Value::Bool(false), |output| Value::from(output.data)))
}

fn resolve<'a>(
    context: &mut dyn Context,
    arguments: &'a [Value],
    direction: Direction,
) -> Option<Call<'a>> {
    let function = match direction {
        Direction::Encrypt => ENCRYPT,
        Direction::Decrypt => DECRYPT,
    };
    Some(Call {
        function,
        direction,
        method: methods::resolve(context, function, args::bytes(arguments, 1))?,
        arguments,
        flags: args::int(arguments, 3).unwrap_or_default(),
    })
}

impl Call<'_> {
    fn run(
        &self,
        context: &mut dyn Context,
        tag_length: i64,
    ) -> Result<Option<aead::Output>, NativeError> {
        let data = args::bytes(self.arguments, 0);
        let decoded;
        let data = if self.direction == Direction::Decrypt && self.flags & RAW_DATA == 0 {
            decoded = decode_base64(data, false)?.unwrap_or_default();
            &decoded
        } else {
            data
        };
        let iv = args::bytes(self.arguments, 4);
        let mut padded_iv = [0; 16];
        let iv = if self.method.mode.authenticated() {
            if !self.validate_aead(context, iv, tag_length) {
                return Ok(None);
            }
            iv
        } else {
            self.block_iv(context, iv, &mut padded_iv)
        };
        let mut padded_key = [0; 32];
        let Some(key) = self.key(context, &mut padded_key) else {
            return Ok(None);
        };
        if self.method.mode.authenticated() {
            Ok(self.authenticated(context, key, iv, data, tag_length))
        } else {
            let padding = if self.flags & ZERO_PADDING == 0 {
                Padding::Pkcs7
            } else {
                Padding::None
            };
            let output = block::apply(self.method, self.direction, padding, key, iv, data);
            if output.is_none() {
                errors::push(context, block::failure(self.direction, data));
            }
            Ok(output.map(|data| aead::Output { data, tag: None }))
        }
    }

    fn key<'a>(&self, context: &mut dyn Context, buffer: &'a mut [u8; 32]) -> Option<&'a [u8]> {
        let key = args::bytes(self.arguments, 2);
        let expected = self.method.key_length();
        let reject = key.len() < expected && self.flags & DONT_ZERO_PAD_KEY != 0;
        if reject || key.len() > expected {
            let error = if self.method.mode == Mode::ChaCha {
                errors::INVALID_CHACHA_KEY
            } else {
                errors::INVALID_KEY
            };
            errors::push(context, error);
        }
        if reject {
            self.warning(context, "Key length cannot be set for the cipher algorithm");
            return None;
        }
        let size = expected.min(key.len());
        buffer[..size].copy_from_slice(&key[..size]);
        Some(&buffer[..expected])
    }

    fn block_iv<'a>(
        &self,
        context: &mut dyn Context,
        iv: &[u8],
        buffer: &'a mut [u8; 16],
    ) -> &'a [u8] {
        let expected = self.method.iv_length();
        let size = iv.len();
        if size == 0 && expected > 0 && self.direction == Direction::Encrypt {
            self.warning(
                context,
                concat!(
                    "Using an empty Initialization Vector (iv) is potentially ",
                    "insecure and not recommended"
                ),
            );
        } else if size > 0 && size < expected {
            self.warning(
                context,
                &format!(
                    "IV passed is only {size} bytes long, cipher expects an IV of precisely \
                 {expected} bytes, padding with \\0"
                ),
            );
        } else if size > expected {
            self.warning(
                context,
                &format!(
                    "IV passed is {size} bytes long which is longer than the {expected} expected \
                 by selected cipher, truncating"
                ),
            );
        }
        if self.direction == Direction::Decrypt && !args::bytes(self.arguments, 5).is_empty() {
            self.warning(
                context,
                "The tag cannot be used because the cipher algorithm does not support AEAD",
            );
        }
        let used = size.min(expected);
        buffer[..used].copy_from_slice(&iv[..used]);
        &buffer[..expected]
    }

    fn validate_aead(&self, context: &mut dyn Context, iv: &[u8], tag_length: i64) -> bool {
        let valid_iv = match self.method.mode {
            Mode::Gcm => (1..=128).contains(&iv.len()),
            Mode::Ccm => (7..=13).contains(&iv.len()),
            Mode::ChaCha => iv.len() == 12,
            _ => true,
        };
        if !valid_iv {
            errors::push(context, errors::INVALID_IV);
            self.warning(context, "Setting of IV length for AEAD mode failed");
            return false;
        }
        if self.direction == Direction::Encrypt {
            if self.method.mode == Mode::Ccm && !ccm_tag_length(tag_length) {
                errors::push(context, errors::INVALID_TAG_LENGTH);
                self.warning(context, "Setting tag length for AEAD cipher failed");
                return false;
            }
        } else {
            let length = args::bytes(self.arguments, 5).len();
            let invalid = length > 16
                || length > 0 && self.method.mode == Mode::Ccm && !ccm_tag_length(length as i64);
            if invalid {
                if self.method.mode == Mode::Gcm {
                    errors::push(
                        context,
                        "error:07800074:common libcrypto routines::too small buffer",
                    );
                    errors::push(
                        context,
                        "error:1C800067:Provider routines::failed to get parameter",
                    );
                } else {
                    errors::push(context, errors::INVALID_TAG_LENGTH);
                }
                self.warning(context, "Setting tag for AEAD cipher decryption failed");
                return false;
            }
        }
        true
    }

    fn authenticated(
        &self,
        context: &mut dyn Context,
        key: &[u8],
        nonce: &[u8],
        data: &[u8],
        tag_length: i64,
    ) -> Option<aead::Output> {
        let mode = self.method.mode;
        let operation = match self.direction {
            Direction::Encrypt => {
                if !args::given(self.arguments, 5) {
                    self.warning(context, "A tag should be provided when using AEAD mode");
                    return None;
                }
                if !(1..=16).contains(&tag_length) {
                    let error = if mode == Mode::Gcm {
                        errors::INVALID_TAG
                    } else {
                        errors::INVALID_TAG_LENGTH
                    };
                    errors::push(context, error);
                    self.warning(context, "Retrieving verification tag failed");
                    return None;
                }
                Operation::Encrypt(tag_length as usize)
            }
            Direction::Decrypt => {
                let tag = args::bytes(self.arguments, 5);
                if tag.is_empty() {
                    errors::push(context, "error:1C800077:Provider routines::tag not set");
                    if mode == Mode::Ccm {
                        errors::push(
                            context,
                            "error:1C800066:Provider routines::cipher operation failed",
                        );
                    }
                    return None;
                }
                Operation::Decrypt(tag)
            }
        };
        let input = Input {
            key,
            nonce,
            data,
            aad: args::bytes(self.arguments, 6),
            operation,
        };
        match aead::apply(self.method, input) {
            Ok(output) => Some(output),
            Err(failure) => {
                match failure {
                    aead::Failure::UnsupportedNonce => {
                        self.warning(context, "Phera does not support this IV length")
                    }
                    aead::Failure::UnsupportedTag => {
                        self.warning(context, "Phera does not support decrypting this tag length")
                    }
                    aead::Failure::Crypto if mode == Mode::Ccm => errors::push(
                        context,
                        "error:1C800066:Provider routines::cipher operation failed",
                    ),
                    aead::Failure::Crypto => errors::push(context, errors::BAD_DECRYPT),
                }
                None
            }
        }
    }

    fn warning(&self, context: &mut dyn Context, message: &str) {
        context.warning(format!("{}(): {message}", self.function));
    }
}

fn ccm_tag_length(length: i64) -> bool {
    (4..=16).contains(&length) && length % 2 == 0
}
