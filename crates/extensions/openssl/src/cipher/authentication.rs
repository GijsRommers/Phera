//! `openssl_decrypt($data, 'aes-256-gcm', $key, 1, $iv, $tag)` checks its tag.

use phera_extension_api::{Context, args};

use super::Call;
use crate::aead::{self, Input, Operation};
use crate::block::Direction;
use crate::errors;
use crate::methods::Mode;

impl Call<'_> {
    pub(super) fn validate_aead(
        &self,
        context: &mut dyn Context,
        iv: &[u8],
        tag_length: i64,
    ) -> bool {
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

    pub(super) fn authenticated(
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
}

fn ccm_tag_length(length: i64) -> bool {
    (4..=16).contains(&length) && length % 2 == 0
}
