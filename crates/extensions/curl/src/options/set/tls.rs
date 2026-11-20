//! Certificate and trust options are part of the connection-pool identity.

use phera_extension_api::{Context, NativeError};

use crate::{
    constants::{ids::*, option_name},
    options::{TlsFiles, TlsFormat, path},
};

use super::{Change, strings};

pub(super) fn is_blob(option: i64) -> bool {
    matches!(
        option,
        CURLOPT_SSLCERT_BLOB | CURLOPT_SSLKEY_BLOB | CURLOPT_CAINFO_BLOB
    )
}

pub(super) fn takes_text(option: i64) -> bool {
    matches!(
        option,
        CURLOPT_CAINFO
            | CURLOPT_CAPATH
            | CURLOPT_SSLCERT
            | CURLOPT_SSLCERTTYPE
            | CURLOPT_SSLKEY
            | CURLOPT_SSLKEYTYPE
            | CURLOPT_KEYPASSWD
            | CURLOPT_PINNEDPUBLICKEY
    )
}

pub(super) fn change(
    context: &mut dyn Context,
    files: &mut TlsFiles,
    option: i64,
    bytes: Option<Vec<u8>>,
) -> Result<Change, NativeError> {
    match option {
        CURLOPT_CAINFO | CURLOPT_CAPATH | CURLOPT_SSLCERT | CURLOPT_SSLKEY => {
            let target = match option {
                CURLOPT_CAINFO => &mut files.ca_file,
                CURLOPT_CAPATH => &mut files.ca_dir,
                CURLOPT_SSLCERT => &mut files.cert,
                _ => &mut files.key,
            };
            *target = bytes
                .map(|bytes| path(context, &bytes))
                .transpose()
                .map_err(|_| {
                    let name = option_name(option).unwrap_or_default();
                    NativeError::value_error(format!("Cannot resolve {name}"))
                })?;
        }
        CURLOPT_CAINFO_BLOB => files.ca_blob = bytes.map(Into::into),
        CURLOPT_SSLCERT_BLOB => files.cert_blob = bytes.map(Into::into),
        CURLOPT_SSLKEY_BLOB => files.key_blob = bytes.map(Into::into),
        CURLOPT_KEYPASSWD => files.key_password = bytes.map(Into::into),
        CURLOPT_SSLCERTTYPE | CURLOPT_SSLKEYTYPE => {
            let format = match bytes.as_deref() {
                None => TlsFormat::Pem,
                Some(value) if value.eq_ignore_ascii_case(b"PEM") => TlsFormat::Pem,
                Some(value) if value.eq_ignore_ascii_case(b"DER") => TlsFormat::Der,
                _ => return Ok(Change::Refused("only PEM and DER are supported")),
            };
            if option == CURLOPT_SSLCERTTYPE {
                files.cert_type = format;
            } else {
                files.key_type = format;
            }
        }
        CURLOPT_PINNEDPUBLICKEY => {
            let pins = strings::text(bytes);
            if pins
                .as_ref()
                .is_some_and(|value| !value.starts_with("sha256//"))
            {
                return Ok(Change::Refused(
                    "only sha256// public key pins are supported",
                ));
            }
            files.pins = pins.map(Into::into);
        }
        _ => return Ok(Change::Refused("unsupported TLS option")),
    }
    Ok(Change::Applied)
}
